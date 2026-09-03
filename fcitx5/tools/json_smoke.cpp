/*
 * SPDX-License-Identifier: MIT
 *
 * json_smoke.cpp - standalone (fcitx5-free) test of PskkClient against a
 * running pskk-server. Useful for debugging the JSON protocol and for
 * exercising the exact request path the Fcitx addon uses.
 *
 * Build:
 *   g++ -std=c++17 -Ifcitx5/src \
 *       fcitx5/src/pskkjson.cpp fcitx5/src/pskkclient.cpp \
 *       fcitx5/tools/json_smoke.cpp -o /tmp/pskk_smoke
 *
 * Run (server must be reachable on PSKK_JSON_ADDR, default 127.0.0.1:50052):
 *   /tmp/pskk_smoke
 */
#include <cstdio>
#include <string>

#include "pskkclient.h"

int main() {
    using namespace pskk;

    PskkClient client;
    std::string error;

    if (!client.ensureConnected(30, 200)) {
        std::fprintf(stderr, "FAIL: could not connect to pskk-server\n");
        return 1;
    }
    std::printf("connected\n");

    int mode = -1;
    if (!client.getMode(&mode, &error)) {
        std::fprintf(stderr, "FAIL getMode: %s\n", error.c_str());
        return 1;
    }
    std::printf("getMode -> %d\n", mode);

    EngineOutput out;
    if (!client.setMode(kHiragana, &out, &error)) {
        std::fprintf(stderr, "FAIL setMode: %s\n", error.c_str());
        return 1;
    }
    std::printf("setMode(1) -> current_mode=%d\n", out.currentMode);

    auto sendKey = [&](const char *name, const char *chr, bool pressed) {
        KeyInput key;
        key.keyName = name;
        key.keyChar = chr ? chr : "";
        key.isPressed = pressed;
        if (!client.processKey(key, &out, &error)) {
            std::fprintf(stderr, "FAIL processKey %s: %s\n", name,
                         error.c_str());
            std::exit(2);
        }
        std::string preedit;
        for (const auto &seg : out.preeditSegments)
            preedit += seg.text;
        std::printf("key %-6s -> consumed=%d preedit='%s' commit='%s'\n", name,
                    out.consumed ? 1 : 0, preedit.c_str(),
                    out.commitString.c_str());
        if (out.status == kHenkanUnavailable)
            std::printf("  (status: HENKAN_UNAVAILABLE)\n");
    };

    sendKey("a", "a", true);
    sendKey("a", "a", false);
    sendKey("i", "i", true);
    sendKey("i", "i", false);

    sendKey("space", " ", true);
    sendKey("space", " ", false);

    if (out.showCandidates) {
        std::printf("candidates (%d, cursor %d):\n", (int)out.candidates.size(),
                    out.candidateCursorPos);
        for (const auto &cand : out.candidates)
            std::printf("  %s\n", cand.surface.c_str());
    }

    uint32_t dictSize = 0;
    if (!client.dictionarySize(&dictSize, &error)) {
        std::fprintf(stderr, "FAIL dictionarySize: %s\n", error.c_str());
        return 1;
    }
    std::printf("dictionary_size -> %u\n", dictSize);

    if (!client.reset(&error)) {
        std::fprintf(stderr, "FAIL reset: %s\n", error.c_str());
        return 1;
    }
    if (!client.focusOut(&out, &error)) {
        std::fprintf(stderr, "FAIL focusOut: %s\n", error.c_str());
        return 1;
    }
    std::printf("focusOut -> commit='%s'\n", out.commitString.c_str());

    std::printf("OK\n");
    return 0;
}
