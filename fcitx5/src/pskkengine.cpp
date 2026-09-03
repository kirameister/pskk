/*
 * SPDX-License-Identifier: MIT
 *
 * pskkengine.cpp - implementation of the PSKK Fcitx 5 engine (see header).
 */
#include "pskkengine.h"

#include <fcntl.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

#include <chrono>
#include <cstdlib>
#include <thread>

#include <fcitx/action.h>
#include <fcitx/addonfactory.h>
#include <fcitx/addonmanager.h>
#include <fcitx/candidatelist.h>
#include <fcitx/event.h>
#include <fcitx/inputcontext.h>
#include <fcitx/inputpanel.h>
#include <fcitx/instance.h>
#include <fcitx/menu.h>
#include <fcitx/statusarea.h>
#include <fcitx/text.h>
#include <fcitx/userinterface.h>
#include <fcitx/userinterfacemanager.h>
#include <fcitx-utils/key.h>
#include <fcitx-utils/keysym.h>

#include "pskkclient.h"

namespace pskk {

namespace {

// Matches IBus: preedit cursor from the server is expressed in characters
// (code points); fcitx5's Text cursor is expressed in bytes.
size_t utf8CharCount(const std::string &s) {
    size_t count = 0;
    for (unsigned char c : s) {
        if ((c & 0xC0) != 0x80)
            count++;
    }
    return count;
}

// Byte offset of the first `chars` code points inside `s`.
size_t byteOffsetForChars(const std::string &s, size_t chars) {
    size_t seen = 0;
    size_t offset = 0;
    while (offset < s.size() && seen < chars) {
        unsigned char c = static_cast<unsigned char>(s[offset]);
        size_t width = 1;
        if (c >= 0xF0)
            width = 4;
        else if (c >= 0xE0)
            width = 3;
        else if (c >= 0xC0)
            width = 2;
        offset += width;
        seen++;
    }
    return offset;
}

}  // namespace

PskkEngine::PskkEngine(fcitx::Instance *instance)
    : instance_(instance), client_(std::make_unique<PskkClient>()) {
    // UI actions mirroring the IBus property list.
    modeAction_ = std::make_unique<fcitx::SimpleAction>();
    hiraganaAction_ = std::make_unique<fcitx::SimpleAction>();
    alphanumericAction_ = std::make_unique<fcitx::SimpleAction>();
    settingsAction_ = std::make_unique<fcitx::SimpleAction>();
    dictionaryEditorAction_ = std::make_unique<fcitx::SimpleAction>();
    menu_ = std::make_unique<fcitx::Menu>();

    modeAction_->setShortText("あ");
    modeAction_->setLongText("Input Mode");
    hiraganaAction_->setShortText("Hiragana");
    hiraganaAction_->setChecked(true);
    alphanumericAction_->setShortText("Alphanumeric");
    alphanumericAction_->setChecked(false);
    settingsAction_->setShortText("Settings");
    dictionaryEditorAction_->setShortText("Dictionary Editor");

    modeAction_->setMenu(menu_.get());
    menu_->addAction(hiraganaAction_.get());
    menu_->addAction(alphanumericAction_.get());
    menu_->addAction(settingsAction_.get());
    menu_->addAction(dictionaryEditorAction_.get());

    instance_->userInterfaceManager().registerAction("pskk-input-mode",
                                                     modeAction_.get());
    instance_->userInterfaceManager().registerAction("pskk-input-mode-hiragana",
                                                     hiraganaAction_.get());
    instance_->userInterfaceManager().registerAction(
        "pskk-input-mode-alphanumeric", alphanumericAction_.get());
    instance_->userInterfaceManager().registerAction("pskk-settings",
                                                     settingsAction_.get());
    instance_->userInterfaceManager().registerAction(
        "pskk-dictionary-editor", dictionaryEditorAction_.get());

    hiraganaAction_->connect<fcitx::SimpleAction::Activated>(
        [this](fcitx::InputContext *ic) { setServerMode(kHiragana, ic); });
    alphanumericAction_->connect<fcitx::SimpleAction::Activated>(
        [this](fcitx::InputContext *ic) { setServerMode(kAlphanumeric, ic); });
    settingsAction_->connect<fcitx::SimpleAction::Activated>(
        [this](fcitx::InputContext * /*ic*/) { launch("pskk-settings"); });
    dictionaryEditorAction_->connect<fcitx::SimpleAction::Activated>(
        [this](fcitx::InputContext * /*ic*/) {
            launch("/opt/pskk/bin/pskk-dictionary-editor");
        });

    // Connect to pskk-server in the background (auto-starting it if needed),
    // reset the engine to Hiragana like the IBus client does at startup, and
    // poll until the henkan dictionary is ready so the first key event does
    // not block the fcitx main loop waiting for HENKAN_UNAVAILABLE to clear.
    warmupThread_ = std::thread([this]() {
        std::string error;
        if (!client_->ensureConnected(30, 150))
            return;
        EngineOutput output;
        if (client_->setMode(kHiragana, &output, &error)) {
            currentMode_.store(output.currentMode);
            initialModeSet_.store(true);
        }
        uint32_t size = 0;
        for (int i = 0; i < 12; ++i) {
            if (client_->dictionarySize(&size, &error) && size > 0)
                break;
            std::this_thread::sleep_for(std::chrono::milliseconds(250));
        }
    });
}

PskkEngine::~PskkEngine() {
    if (warmupThread_.joinable())
        warmupThread_.join();

    instance_->userInterfaceManager().unregisterAction(dictionaryEditorAction_.get());
    instance_->userInterfaceManager().unregisterAction(settingsAction_.get());
    instance_->userInterfaceManager().unregisterAction(alphanumericAction_.get());
    instance_->userInterfaceManager().unregisterAction(hiraganaAction_.get());
    instance_->userInterfaceManager().unregisterAction(modeAction_.get());
}

// ---------------------------------------------------------------------------
// Key event handling
// ---------------------------------------------------------------------------

bool PskkEngine::translateKey(const fcitx::KeyEvent &keyEvent,
                              KeyInput *input) const {
    // Use the raw (post-layout-conversion, pre-normalization) key: the closest
    // analog to the keyvals the IBus client received from X, so the server
    // sees the same key_name/key_char/modifiers under both frameworks.
    const fcitx::Key rawKey = keyEvent.rawKey();
    const fcitx::KeySym sym = rawKey.sym();
    const fcitx::KeyStates states = rawKey.states();

    input->isPressed = !keyEvent.isRelease();
    input->keyName = fcitx::Key::keySymToString(sym);

    const uint32_t uni = fcitx::Key::keySymToUnicode(sym);
    input->keyChar =
        (uni >= 0x20 && uni < 0x7F) ? std::string(1, static_cast<char>(uni)) : std::string();

    input->modifiers.shift = states.test(fcitx::KeyState::Shift);
    input->modifiers.ctrl = states.test(fcitx::KeyState::Ctrl);
    input->modifiers.alt = states.test(fcitx::KeyState::Alt);
    input->modifiers.super_ = states.test(fcitx::KeyState::Super);
    return true;
}

bool PskkEngine::processKeyWithRetry(fcitx::InputContext *ic,
                                     const KeyInput &input) {
    EngineOutput output;
    std::string error;
    if (!client_->processKey(input, &output, &error)) {
        // Server unreachable: pass the key through instead of freezing input.
        // The next key event will try to reconnect.
        return false;
    }

    // While the henkan dictionary is still loading the server reports
    // HENKAN_UNAVAILABLE. The IBus client retries for up to ~10 s in its own
    // process; an in-process fcitx addon must not block the daemon that long,
    // so bound the retry window. Like the IBus client, keys typed during the
    // loading window are consumed (suppressed), not leaked into the app.
    int retries = 0;
    while (output.status == kHenkanUnavailable && retries < 20) {
        std::this_thread::sleep_for(std::chrono::milliseconds(50));
        if (!client_->processKey(input, &output, &error)) {
            return false;
        }
        retries++;
    }

    render(ic, output);
    if (output.status == kHenkanUnavailable) {
        // Still loading after the bounded retry window: suppress the key,
        // matching the IBus client ("Suppress the key as requested").
        return true;
    }
    return output.consumed;
}

void PskkEngine::keyEvent(const fcitx::InputMethodEntry &entry,
                          fcitx::KeyEvent &keyEvent) {
    (void)entry;
    fcitx::InputContext *ic = keyEvent.inputContext();
    if (ic == nullptr)
        return;

    KeyInput input;
    if (!translateKey(keyEvent, &input))
        return;

    // Track the Super key manually, exactly like the IBus client: Super
    // combos are system shortcuts (e.g. Super+Space to switch IMEs), so while
    // Super is held everything is passed through untouched.
    const std::string &name = input.keyName;
    if (name == "Super_L" || name == "Super_R" || name == "Super") {
        superPressed_.store(input.isPressed);
        return;  // do not forward, do not consume
    }
    if (superPressed_.load())
        return;  // Super is held: pass everything through

    // Lazy reconnect in case the warm-up failed (e.g. server started later).
    if (!client_->ensureConnected(5, 100)) {
        return;  // not consumed -> key goes to the application
    }

    if (processKeyWithRetry(ic, input)) {
        keyEvent.filter();  // consumed by the engine
    }
}

// ---------------------------------------------------------------------------
// Output rendering (IBus _update_ui equivalent)
// ---------------------------------------------------------------------------

void PskkEngine::render(fcitx::InputContext *ic, const EngineOutput &output) {
    if (ic == nullptr)
        return;

    // Commit first: the application replaces the selected text, mirroring the
    // order in which commits and preedit updates reach the client.
    if (!output.commitString.empty()) {
        ic->commitString(output.commitString);
    }

    fcitx::InputPanel &panel = ic->inputPanel();
    panel.reset();

    if (!output.preeditSegments.empty()) {
        fcitx::Text text;
        size_t charPos = 0;
        size_t byteCursor = 0;
        bool cursorFound = false;
        for (const auto &seg : output.preeditSegments) {
            fcitx::TextFormatFlags flag = fcitx::TextFormatFlag::Underline;
            if (seg.isSelected) {
                // Selected (converting) segment: highlight instead of underline.
                flag = fcitx::TextFormatFlag::HighLight;
            }
            text.append(seg.text, flag);

            if (!cursorFound) {
                const size_t segChars = utf8CharCount(seg.text);
                if (charPos + segChars <=
                    static_cast<size_t>(output.preeditCursorPos)) {
                    byteCursor += seg.text.size();
                } else {
                    byteCursor += byteOffsetForChars(
                        seg.text,
                        static_cast<size_t>(output.preeditCursorPos) - charPos);
                    cursorFound = true;
                }
                charPos += segChars;
            }
        }
        text.setCursor(static_cast<int>(byteCursor));
        panel.setPreedit(text);
    }

    if (output.showCandidates && !output.candidates.empty()) {
        auto candidateList = std::make_unique<fcitx::CommonCandidateList>();
        candidateList->setPageSize(9);  // IBus lookup table page size
        int index = 0;
        for (const auto &candidate : output.candidates) {
            // DisplayOnlyCandidateWord: clicking a candidate is a no-op, which
            // matches the IBus client (candidate clicks are not wired there
            // either); selection is driven entirely by key events.
            candidateList->insert(
                index++,
                std::make_unique<fcitx::DisplayOnlyCandidateWord>(
                    fcitx::Text(candidate.surface)));
        }
        if (output.candidateCursorPos >= 0 &&
            output.candidateCursorPos < static_cast<int>(output.candidates.size())) {
            candidateList->setGlobalCursorIndex(output.candidateCursorPos);
        }
        panel.setCandidateList(std::move(candidateList));
    }

    const int mode = output.currentMode;
    if (mode != currentMode_.load()) {
        currentMode_.store(mode);
        updateModeActions(ic);
        // Show the mode change like the IBus property icon update does.
        instance_->showInputMethodInformation(ic);
    }

    ic->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

void PskkEngine::activate(const fcitx::InputMethodEntry &entry,
                          fcitx::InputContextEvent &event) {
    (void)entry;
    fcitx::InputContext *ic = event.inputContext();
    if (ic == nullptr)
        return;
    ic->statusArea().addAction(fcitx::StatusGroup::InputMethod,
                               modeAction_.get());

    // Guarantee the engine starts in Hiragana, like the IBus client does at
    // startup, even if a key event arrives before the background warm-up
    // finished.
    if (!initialModeSet_.load() && client_->ensureConnected(10, 100)) {
        std::string error;
        EngineOutput output;
        if (client_->setMode(kHiragana, &output, &error)) {
            currentMode_.store(output.currentMode);
            initialModeSet_.store(true);
        }
    }
    updateModeActions(ic);
}

void PskkEngine::deactivate(const fcitx::InputMethodEntry &entry,
                            fcitx::InputContextEvent &event) {
    (void)entry;
    fcitx::InputContext *ic = event.inputContext();
    if (ic == nullptr)
        return;
    // Mirror the IBus client's do_focus_out: tell the server the context lost
    // focus; it commits any pending preedit and clears its state.
    std::string error;
    EngineOutput output;
    if (client_->focusOut(&output, &error)) {
        render(ic, output);
    }
}

void PskkEngine::reset(const fcitx::InputMethodEntry &entry,
                       fcitx::InputContextEvent &event) {
    (void)entry;
    fcitx::InputContext *ic = event.inputContext();
    if (ic == nullptr)
        return;
    std::string error;
    client_->reset(&error);
    clearPanel(ic);
}

void PskkEngine::clearPanel(fcitx::InputContext *ic) {
    ic->inputPanel().reset();
    ic->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
}

// ---------------------------------------------------------------------------
// Mode switching and actions
// ---------------------------------------------------------------------------

void PskkEngine::setServerMode(int mode, fcitx::InputContext *ic) {
    std::string error;
    EngineOutput output;
    if (!client_->setMode(mode, &output, &error))
        return;
    currentMode_.store(output.currentMode);
    if (ic != nullptr) {
        render(ic, output);
        updateModeActions(ic);
    }
}

void PskkEngine::updateModeActions(fcitx::InputContext *ic) {
    const bool hiragana = currentMode_.load() == kHiragana;
    modeAction_->setShortText(hiragana ? "あ" : "A");
    modeAction_->setLongText(hiragana ? "Hiragana" : "Alphanumeric");
    hiraganaAction_->setChecked(hiragana);
    alphanumericAction_->setChecked(!hiragana);

    modeAction_->update(ic);
    hiraganaAction_->update(ic);
    alphanumericAction_->update(ic);
}

std::string PskkEngine::subMode(const fcitx::InputMethodEntry &entry,
                                fcitx::InputContext &inputContext) {
    (void)entry;
    (void)inputContext;
    return currentMode_.load() == kHiragana ? "Hiragana" : "Alphanumeric";
}

std::string PskkEngine::subModeLabelImpl(const fcitx::InputMethodEntry &entry,
                                         fcitx::InputContext &inputContext) {
    (void)entry;
    (void)inputContext;
    return currentMode_.load() == kHiragana ? "あ" : "A";
}

void PskkEngine::launch(const char *command) {
    // Double fork so the daemon never accumulates zombies.
    pid_t pid = fork();
    if (pid < 0)
        return;
    if (pid == 0) {
        pid_t grandchild = fork();
        if (grandchild < 0)
            _exit(127);
        if (grandchild == 0) {
            int devnull = open("/dev/null", O_RDWR);
            if (devnull >= 0) {
                dup2(devnull, STDIN_FILENO);
                dup2(devnull, STDOUT_FILENO);
                dup2(devnull, STDERR_FILENO);
                if (devnull > STDERR_FILENO)
                    close(devnull);
            }
            execlp(command, command, static_cast<char *>(nullptr));
            _exit(127);
        }
        _exit(0);
    }
    int status = 0;
    waitpid(pid, &status, 0);
}

fcitx::AddonInstance *PskkAddonFactory::create(fcitx::AddonManager *manager) {
    return new PskkEngine(manager->instance());
}

}  // namespace pskk

FCITX_ADDON_FACTORY(pskk::PskkAddonFactory)
