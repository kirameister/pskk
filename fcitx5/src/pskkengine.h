/*
 * SPDX-License-Identifier: MIT
 *
 * pskkengine.h - Fcitx 5 input method engine for PSKK.
 *
 * Thin adapter between fcitx5 and the PSKK engine server (pskk-server),
 * mirroring the IBus Python client (ibus-engine-pskk.py) 1:1:
 *   - key events -> gRPC/JSON KeyEvent
 *   - EngineOutput -> preedit / commit / candidate panel
 *   - input mode menu (Hiragana/Alphanumeric) as fcitx actions
 *
 * The engine talks to the same server the IBus client uses, but through the
 * lightweight JSON/TCP protocol on 127.0.0.1:50052 (see src/json/server.rs),
 * so the addon has no gRPC/protobuf C++ dependency.
 */
#ifndef PSKK_FCITX5_PSKKENGINE_H_
#define PSKK_FCITX5_PSKKENGINE_H_

#include <atomic>
#include <memory>
#include <string>
#include <thread>

#include <fcitx/addonfactory.h>
#include <fcitx/inputmethodengine.h>

namespace fcitx {
class AddonFactory;
class AddonInstance;
class AddonManager;
class Instance;
class InputContext;
class InputContextEvent;
class InputMethodEntry;
class KeyEvent;
class Menu;
class SimpleAction;
}  // namespace fcitx

namespace pskk {

class PskkClient;
struct KeyInput;
struct EngineOutput;

class PskkEngine : public fcitx::InputMethodEngineV2 {
public:
    explicit PskkEngine(fcitx::Instance *instance);
    ~PskkEngine() override;

    // InputMethodEngine interface
    void keyEvent(const fcitx::InputMethodEntry &entry,
                  fcitx::KeyEvent &keyEvent) override;
    void activate(const fcitx::InputMethodEntry &entry,
                  fcitx::InputContextEvent &event) override;
    void deactivate(const fcitx::InputMethodEntry &entry,
                    fcitx::InputContextEvent &event) override;
    void reset(const fcitx::InputMethodEntry &entry,
               fcitx::InputContextEvent &event) override;
    std::string subMode(const fcitx::InputMethodEntry &entry,
                        fcitx::InputContext &inputContext) override;
    std::string subModeIconImpl(const fcitx::InputMethodEntry &entry,
                                fcitx::InputContext &inputContext) override;
    std::string subModeLabelImpl(const fcitx::InputMethodEntry &entry,
                                 fcitx::InputContext &inputContext) override;

private:
    // Key handling / rendering
    bool translateKey(const fcitx::KeyEvent &keyEvent, KeyInput *input) const;
    bool processKeyWithRetry(fcitx::InputContext *ic, const KeyInput &input);
    void render(fcitx::InputContext *ic, const EngineOutput &output);
    void setServerMode(int mode, fcitx::InputContext *ic);

    // UI helpers
    void updateModeActions(fcitx::InputContext *ic);
    void launch(const char *command);
    static void clearPanel(fcitx::InputContext *ic);

    fcitx::Instance *instance_;
    std::unique_ptr<PskkClient> client_;

    std::unique_ptr<fcitx::Menu> menu_;
    std::unique_ptr<fcitx::SimpleAction> modeAction_;
    std::unique_ptr<fcitx::SimpleAction> hiraganaAction_;
    std::unique_ptr<fcitx::SimpleAction> alphanumericAction_;
    std::unique_ptr<fcitx::SimpleAction> settingsAction_;
    std::unique_ptr<fcitx::SimpleAction> dictionaryEditorAction_;
    std::unique_ptr<fcitx::SimpleAction> imeTesterAction_;

    std::atomic<int> currentMode_{1};  // pskk::kHiragana
    std::atomic<bool> superPressed_{false};
    std::atomic<bool> initialModeSet_{false};
    std::thread warmupThread_;
};

class PskkAddonFactory : public fcitx::AddonFactory {
public:
    fcitx::AddonInstance *create(fcitx::AddonManager *manager) override;
};

}  // namespace pskk

#endif  // PSKK_FCITX5_PSKKENGINE_H_
