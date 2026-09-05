/*
 * SPDX-License-Identifier: MIT
 *
 * pskkclient.h - Minimal TCP/JSON client for the PSKK engine server.
 *
 * Talks the newline-delimited JSON protocol served by pskk-server on
 * 127.0.0.1:50052 (see src/json/server.rs for the full contract). This file
 * intentionally has NO fcitx5 dependency so it can be compiled and tested
 * standalone.
 */
#ifndef PSKK_FCITX5_PSKKCLIENT_H_
#define PSKK_FCITX5_PSKKCLIENT_H_

#include <chrono>
#include <cstdint>
#include <mutex>
#include <string>
#include <vector>

#include "pskkjson.h"

namespace pskk {

// Mirrors pskk.proto InputMode values.
enum InputMode {
    kAlphanumeric = 0,
    kHiragana = 1,
};

// Mirrors pskk.proto ResponseStatus values.
enum ResponseStatus {
    kStatusOk = 0,
    kHenkanUnavailable = 1,
};

struct KeyModifiers {
    bool shift = false;
    bool ctrl = false;
    bool alt = false;
    bool super_ = false;
};

struct KeyInput {
    std::string keyChar;    // single ASCII printable char, else empty
    std::string keyName;    // e.g. "a", "space", "Return", "Henkan"
    bool isPressed = true;  // false for key release
    KeyModifiers modifiers;
};

struct PreeditSegment {
    std::string text;
    bool isSelected = false;
};

struct Candidate {
    std::string surface;
    std::string reading;
};

// Mirrors pskk.proto EngineOutput.
struct EngineOutput {
    std::string commitString;
    std::vector<PreeditSegment> preeditSegments;
    int preeditCursorPos = 0;      // in characters (code points)
    std::vector<Candidate> candidates;
    int candidateCursorPos = 0;
    bool showCandidates = false;
    bool consumed = false;
    int currentMode = kAlphanumeric;
    int markerState = 0;
    int engineState = 0;
    int status = kStatusOk;
};

// Parse an EngineOutput JSON object (used by the server response path).
bool parseEngineOutput(const Json &json, EngineOutput *out);

class PskkClient {
public:
    PskkClient();
    ~PskkClient();

    PskkClient(const PskkClient &) = delete;
    PskkClient &operator=(const PskkClient &) = delete;

    /// True once a connection to the server exists.
    bool connected() const;

    /// Connect to pskk-server, auto-starting it if needed. Tries up to
    /// `maxAttempts` with `delayMs` between attempts. Thread-safe.
    bool ensureConnected(int maxAttempts = 3, int delayMs = 200);

    /// Send one key event and read back the EngineOutput.
    bool processKey(const KeyInput &key, EngineOutput *out, std::string *error);

    bool setMode(int mode, EngineOutput *out, std::string *error);
    bool getMode(int *mode, std::string *error);
    bool focusOut(EngineOutput *out, std::string *error);
    bool reset(std::string *error);
    bool dictionarySize(uint32_t *size, std::string *error);

    void shutdown();

private:
    bool connectLocked(std::string *error);
    void closeLocked();
    bool sendAndReceiveLocked(const Json &request, Json *response,
                              std::string *error);
    bool spawnAllowedLocked() const;
    void spawnServerLocked();

    int fd_ = -1;
    bool everSpawned_ = false;
    std::chrono::steady_clock::time_point lastSpawnAttempt_{};
    mutable std::mutex mutex_;
};

}  // namespace pskk

#endif  // PSKK_FCITX5_PSKKCLIENT_H_
