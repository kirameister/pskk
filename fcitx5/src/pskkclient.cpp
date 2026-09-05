/*
 * SPDX-License-Identifier: MIT
 *
 * pskkclient.cpp - implementation of PskkClient (see pskkclient.h).
 */
#include "pskkclient.h"

#include <arpa/inet.h>
#include <cerrno>
#include <cstdlib>
#include <cstring>
#include <fcntl.h>
#include <netdb.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#include <poll.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

#include <chrono>
#include <thread>

namespace pskk {

namespace {

constexpr int kConnectTimeoutMs = 1000;
constexpr int kIoTimeoutSec = 5;
constexpr size_t kMaxResponseBytes = 512 * 1024;

struct Endpoint {
    std::string host;
    int port;
};

Endpoint defaultEndpoint() {
    Endpoint ep{"127.0.0.1", 50052};
    const char *env = std::getenv("PSKK_JSON_ADDR");
    if (env == nullptr || *env == '\0')
        return ep;
    std::string value(env);
    size_t colon = value.rfind(':');
    if (colon == std::string::npos)
        return ep;
    std::string portStr = value.substr(colon + 1);
    char *end = nullptr;
    long port = std::strtol(portStr.c_str(), &end, 10);
    if (end == nullptr || *end != '\0' || port <= 0 || port > 65535)
        return ep;
    ep.host = value.substr(0, colon);
    ep.port = static_cast<int>(port);
    return ep;
}

bool resolveEndpoint(const Endpoint &ep, sockaddr_in *out) {
    std::memset(out, 0, sizeof(*out));
    out->sin_family = AF_INET;
    out->sin_port = htons(static_cast<uint16_t>(ep.port));
    if (inet_pton(AF_INET, ep.host.c_str(), &out->sin_addr) == 1)
        return true;
    addrinfo hints{};
    hints.ai_family = AF_INET;
    hints.ai_socktype = SOCK_STREAM;
    addrinfo *result = nullptr;
    if (getaddrinfo(ep.host.c_str(), nullptr, &hints, &result) != 0 ||
        result == nullptr) {
        if (result)
            freeaddrinfo(result);
        return false;
    }
    std::memcpy(&out->sin_addr,
                &reinterpret_cast<sockaddr_in *>(result->ai_addr)->sin_addr,
                sizeof(out->sin_addr));
    freeaddrinfo(result);
    return true;
}

bool hasExecutable(const std::string &path) {
    return access(path.c_str(), X_OK) == 0;
}

std::vector<std::string> serverCandidates() {
    std::vector<std::string> candidates;
    if (const char *explicitPath = std::getenv("PSKK_SERVER")) {
        if (*explicitPath)
            candidates.emplace_back(explicitPath);
    }
    if (const char *root = std::getenv("PSKK_INSTALL_ROOT")) {
        candidates.emplace_back(std::string(root) + "/bin/pskk-server");
    }
    candidates.emplace_back("/opt/pskk/bin/pskk-server");
    candidates.emplace_back("target/release/pskk-server");
    candidates.emplace_back("target/debug/pskk-server");
    candidates.emplace_back("/usr/local/bin/pskk-server");
    return candidates;
}

void setNonBlocking(int fd, bool enabled) {
    int flags = fcntl(fd, F_GETFL, 0);
    if (flags < 0)
        return;
    fcntl(fd, F_SETFL, enabled ? (flags | O_NONBLOCK) : (flags & ~O_NONBLOCK));
}

}  // namespace

bool parseEngineOutput(const Json &json, EngineOutput *out) {
    if (!json.isObject())
        return false;
    EngineOutput result;

    auto str = [&json](const char *key, const std::string &fallback) {
        const Json *v = json.find(key);
        if (v == nullptr || !v->isString())
            return fallback;
        return v->asString();
    };
    auto integer = [&json](const char *key, int fallback) {
        const Json *v = json.find(key);
        if (v == nullptr || !v->isNumber())
            return fallback;
        return static_cast<int>(v->asInt());
    };
    auto boolean = [&json](const char *key, bool fallback) {
        const Json *v = json.find(key);
        if (v == nullptr || !v->isBool())
            return fallback;
        return v->asBool();
    };

    result.commitString = str("commit_string", "");
    result.preeditCursorPos = integer("preedit_cursor_pos", 0);
    result.candidateCursorPos = integer("candidate_cursor_pos", 0);
    result.showCandidates = boolean("show_candidates", false);
    result.consumed = boolean("consumed", false);
    result.currentMode = integer("current_mode", kAlphanumeric);
    result.markerState = integer("marker_state", 0);
    result.engineState = integer("engine_state", 0);
    result.status = integer("status", kStatusOk);

    if (const Json *segments = json.find("preedit_segments");
        segments != nullptr && segments->isArray()) {
        result.preeditSegments.reserve(segments->size());
        for (size_t i = 0; i < segments->size(); ++i) {
            const Json &seg = segments->at(i);
            PreeditSegment outSeg;
            if (const Json *text = seg.find("text"); text && text->isString())
                outSeg.text = text->asString();
            if (const Json *sel = seg.find("is_selected");
                sel && sel->isBool())
                outSeg.isSelected = sel->asBool();
            result.preeditSegments.push_back(std::move(outSeg));
        }
    }

    if (const Json *cands = json.find("candidates");
        cands != nullptr && cands->isArray()) {
        result.candidates.reserve(cands->size());
        for (size_t i = 0; i < cands->size(); ++i) {
            const Json &cand = cands->at(i);
            Candidate outCand;
            if (const Json *surface = cand.find("surface");
                surface && surface->isString())
                outCand.surface = surface->asString();
            if (const Json *reading = cand.find("reading");
                reading && reading->isString())
                outCand.reading = reading->asString();
            result.candidates.push_back(std::move(outCand));
        }
    }

    *out = std::move(result);
    return true;
}

PskkClient::PskkClient() = default;
PskkClient::~PskkClient() { shutdown(); }

bool PskkClient::connected() const {
    std::lock_guard<std::mutex> lock(mutex_);
    return fd_ >= 0;
}

bool PskkClient::ensureConnected(int maxAttempts, int delayMs) {
    std::lock_guard<std::mutex> lock(mutex_);
    if (fd_ >= 0)
        return true;

    // First: is a server already running?
    {
        std::string error;
        if (connectLocked(&error))
            return true;
    }

    // No: (re)start one. The addon is long-lived, so spawning is not limited
    // to "once per process": when the connection is gone (e.g. the server was
    // pkill'ed) the next ensureConnected() will spawn again. Only the rate is
    // throttled so a dead server is not respawned in a tight loop.
    if (spawnAllowedLocked()) {
        spawnServerLocked();
    }

    for (int attempt = 0; attempt < maxAttempts; ++attempt) {
        std::string error;
        if (connectLocked(&error))
            return true;
        if (attempt + 1 < maxAttempts)
            std::this_thread::sleep_for(std::chrono::milliseconds(delayMs));
    }
    return false;
}

bool PskkClient::spawnAllowedLocked() const {
    const auto now = std::chrono::steady_clock::now();
    if (!everSpawned_)
        return true;
    // Throttle respawns: at most one spawn attempt every 1.5 s, so a server
    // that dies (e.g. pkill) is restarted quickly, but a server that crashes
    // on startup is not respawned in a tight loop.
    return now - lastSpawnAttempt_ > std::chrono::milliseconds(1500);
}

void PskkClient::spawnServerLocked() {
    everSpawned_ = true;
    lastSpawnAttempt_ = std::chrono::steady_clock::now();
    for (const std::string &candidate : serverCandidates()) {
        if (!hasExecutable(candidate))
            continue;
        pid_t pid = fork();
        if (pid < 0)
            continue;  // fork failed; try the next candidate
        if (pid == 0) {
            // Child: detach and run the server with no stdio attached.
            setsid();
            int devnull = open("/dev/null", O_RDWR);
            if (devnull >= 0) {
                dup2(devnull, STDIN_FILENO);
                dup2(devnull, STDOUT_FILENO);
                dup2(devnull, STDERR_FILENO);
                if (devnull > STDERR_FILENO)
                    close(devnull);
            }
            char *args[] = {const_cast<char *>(candidate.c_str()), nullptr};
            execv(candidate.c_str(), args);
            _exit(127);
        }
        // Parent: we spawned it; poll for the listener in the retry loop.
        (void)pid;
        break;
    }
}

bool PskkClient::connectLocked(std::string *error) {
    if (fd_ >= 0)
        return true;

    Endpoint ep = defaultEndpoint();
    sockaddr_in addr;
    if (!resolveEndpoint(ep, &addr)) {
        *error = "cannot resolve server address " + ep.host;
        return false;
    }

    int fd = socket(AF_INET, SOCK_STREAM | SOCK_CLOEXEC, 0);
    if (fd < 0) {
        *error = std::string("socket(): ") + std::strerror(errno);
        return false;
    }

    setNonBlocking(fd, true);
    int rc = connect(fd, reinterpret_cast<sockaddr *>(&addr), sizeof(addr));
    if (rc < 0 && errno == EINPROGRESS) {
        pollfd pfd{fd, POLLOUT, 0};
        rc = poll(&pfd, 1, kConnectTimeoutMs);
        if (rc == 0) {
            *error = "connection to server timed out";
            close(fd);
            return false;
        }
        if (rc < 0) {
            *error = std::string("poll(): ") + std::strerror(errno);
            close(fd);
            return false;
        }
        int soError = 0;
        socklen_t len = sizeof(soError);
        if (getsockopt(fd, SOL_SOCKET, SO_ERROR, &soError, &len) < 0 ||
            soError != 0) {
            *error = soError ? std::string("connect(): ") + std::strerror(soError)
                             : "connect failed";
            close(fd);
            return false;
        }
        rc = 0;
    }
    if (rc < 0) {
        *error = std::string("connect(): ") + std::strerror(errno);
        close(fd);
        return false;
    }

    setNonBlocking(fd, false);

    timeval timeout{};
    timeout.tv_sec = kIoTimeoutSec;
    setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &timeout, sizeof(timeout));
    setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &timeout, sizeof(timeout));
    int one = 1;
    setsockopt(fd, IPPROTO_TCP, TCP_NODELAY, &one, sizeof(one));

    fd_ = fd;
    return true;
}

void PskkClient::closeLocked() {
    if (fd_ >= 0) {
        close(fd_);
        fd_ = -1;
    }
}

bool PskkClient::sendAndReceiveLocked(const Json &request, Json *response,
                                      std::string *error) {
    if (fd_ < 0) {
        if (!connectLocked(error))
            return false;
    }

    std::string payload = request.dump();
    payload.push_back('\n');

    size_t sent = 0;
    while (sent < payload.size()) {
        ssize_t n = send(fd_, payload.data() + sent, payload.size() - sent,
                         MSG_NOSIGNAL);
        if (n > 0) {
            sent += static_cast<size_t>(n);
            continue;
        }
        if (n < 0 && (errno == EINTR || errno == EAGAIN))
            continue;
        *error = std::string("send(): ") +
                 (n < 0 ? std::strerror(errno) : "connection closed");
        closeLocked();
        return false;
    }

    std::string buffer;
    buffer.reserve(1024);
    while (true) {
        if (buffer.size() >= kMaxResponseBytes) {
            *error = "response too large";
            closeLocked();
            return false;
        }
        char chunk[4096];
        ssize_t n = recv(fd_, chunk, sizeof(chunk), 0);
        if (n > 0) {
            for (ssize_t i = 0; i < n; ++i) {
                if (chunk[i] == '\n') {
                    buffer.append(chunk, static_cast<size_t>(i));
                    if (!Json::parse(buffer, response, error)) {
                        closeLocked();
                        return false;
                    }
                    if (const Json *errVal = response->find("error");
                        errVal != nullptr && errVal->isString()) {
                        *error = errVal->asString();
                        return false;
                    }
                    return true;
                }
            }
            buffer.append(chunk, static_cast<size_t>(n));
            continue;
        }
        if (n < 0 && (errno == EINTR || errno == EAGAIN))
            continue;
        *error = (n == 0) ? "connection closed by server"
                          : std::string("recv(): ") + std::strerror(errno);
        closeLocked();
        return false;
    }
}

bool PskkClient::processKey(const KeyInput &key, EngineOutput *out,
                            std::string *error) {
    Json modifiers = Json::object();
    modifiers.set("shift", Json::boolean(key.modifiers.shift))
        .set("ctrl", Json::boolean(key.modifiers.ctrl))
        .set("alt", Json::boolean(key.modifiers.alt))
        .set("super", Json::boolean(key.modifiers.super_));

    Json request = Json::object();
    request.set("op", Json::string("process_key"))
        .set("key_char", Json::string(key.keyChar))
        .set("key_name", Json::string(key.keyName))
        .set("is_pressed", Json::boolean(key.isPressed))
        .set("modifiers", std::move(modifiers));

    std::lock_guard<std::mutex> lock(mutex_);
    Json response;
    if (!sendAndReceiveLocked(request, &response, error))
        return false;
    return parseEngineOutput(response, out);
}

bool PskkClient::setMode(int mode, EngineOutput *out, std::string *error) {
    Json request = Json::object();
    request.set("op", Json::string("set_mode"))
        .set("mode", Json::number(static_cast<double>(mode)));

    std::lock_guard<std::mutex> lock(mutex_);
    Json response;
    if (!sendAndReceiveLocked(request, &response, error))
        return false;
    return parseEngineOutput(response, out);
}

bool PskkClient::getMode(int *mode, std::string *error) {
    Json request = Json::object();
    request.set("op", Json::string("get_mode"));

    std::lock_guard<std::mutex> lock(mutex_);
    Json response;
    if (!sendAndReceiveLocked(request, &response, error))
        return false;
    const Json *value = response.find("mode");
    if (value == nullptr || !value->isNumber()) {
        *error = "malformed get_mode response";
        return false;
    }
    *mode = static_cast<int>(value->asInt());
    return true;
}

bool PskkClient::focusOut(EngineOutput *out, std::string *error) {
    Json request = Json::object();
    request.set("op", Json::string("focus_out"));

    std::lock_guard<std::mutex> lock(mutex_);
    Json response;
    if (!sendAndReceiveLocked(request, &response, error))
        return false;
    return parseEngineOutput(response, out);
}

bool PskkClient::reset(std::string *error) {
    Json request = Json::object();
    request.set("op", Json::string("reset"));

    std::lock_guard<std::mutex> lock(mutex_);
    Json response;
    if (!sendAndReceiveLocked(request, &response, error))
        return false;
    return true;
}

bool PskkClient::dictionarySize(uint32_t *size, std::string *error) {
    Json request = Json::object();
    request.set("op", Json::string("get_dictionary_size"));

    std::lock_guard<std::mutex> lock(mutex_);
    Json response;
    if (!sendAndReceiveLocked(request, &response, error))
        return false;
    const Json *value = response.find("size");
    if (value == nullptr || !value->isNumber()) {
        *error = "malformed get_dictionary_size response";
        return false;
    }
    *size = static_cast<uint32_t>(value->asInt());
    return true;
}

void PskkClient::shutdown() {
    std::lock_guard<std::mutex> lock(mutex_);
    closeLocked();
}

}  // namespace pskk
