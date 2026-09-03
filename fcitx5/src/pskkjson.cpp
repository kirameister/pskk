/*
 * SPDX-License-Identifier: MIT
 *
 * pskkjson.cpp - minimal JSON parser/serializer (see pskkjson.h).
 */
#include "pskkjson.h"

#include <cerrno>
#include <cmath>
#include <cstdlib>
#include <cstring>

namespace pskk {

Json Json::boolean(bool b) {
    Json j(Type::Boolean);
    j.boolValue_ = b;
    return j;
}

Json Json::number(double n) {
    Json j(Type::Number);
    j.numberValue_ = n;
    return j;
}

Json Json::string(std::string s) {
    Json j(Type::String);
    j.stringValue_ = std::move(s);
    return j;
}

Json Json::array() { return Json(Type::Array); }

Json Json::object() { return Json(Type::Object); }

const Json *Json::find(const std::string &key) const {
    if (type_ != Type::Object)
        return nullptr;
    for (const auto &member : members_) {
        if (member.first == key)
            return &member.second;
    }
    return nullptr;
}

Json &Json::set(const std::string &key, Json value) {
    if (type_ != Type::Object) {
        *this = Json(Type::Object);
    }
    for (auto &member : members_) {
        if (member.first == key) {
            member.second = std::move(value);
            return *this;
        }
    }
    members_.emplace_back(key, std::move(value));
    return *this;
}

Json &Json::push(Json value) {
    if (type_ != Type::Array) {
        *this = Json(Type::Array);
    }
    items_.push_back(std::move(value));
    return *this;
}

// ---------------------------------------------------------------------------
// Serialization
// ---------------------------------------------------------------------------

namespace {

void escapeInto(std::string *out, const std::string &s) {
    out->push_back('"');
    for (unsigned char c : s) {
        switch (c) {
            case '"':
                out->append("\\\"");
                break;
            case '\\':
                out->append("\\\\");
                break;
            case '\b':
                out->append("\\b");
                break;
            case '\f':
                out->append("\\f");
                break;
            case '\n':
                out->append("\\n");
                break;
            case '\r':
                out->append("\\r");
                break;
            case '\t':
                out->append("\\t");
                break;
            default:
                if (c < 0x20) {
                    char buf[8];
                    std::snprintf(buf, sizeof(buf), "\\u%04x", c);
                    out->append(buf);
                } else {
                    out->push_back(static_cast<char>(c));  // UTF-8 passthrough
                }
        }
    }
    out->push_back('"');
}

}  // namespace

void Json::dumpTo(std::string *out) const {
    switch (type_) {
        case Type::Null:
            out->append("null");
            break;
        case Type::Boolean:
            out->append(boolValue_ ? "true" : "false");
            break;
        case Type::Number: {
            double n = numberValue_;
            if (std::isfinite(n) && n == std::floor(n) && std::fabs(n) < 1e15) {
                out->append(std::to_string(static_cast<int64_t>(n)));
            } else {
                char buf[40];
                std::snprintf(buf, sizeof(buf), "%.17g", n);
                out->append(buf);
            }
            break;
        }
        case Type::String:
            escapeInto(out, stringValue_);
            break;
        case Type::Array: {
            out->push_back('[');
            bool first = true;
            for (const Json &item : items_) {
                if (!first)
                    out->push_back(',');
                first = false;
                item.dumpTo(out);
            }
            out->push_back(']');
            break;
        }
        case Type::Object: {
            out->push_back('{');
            bool first = true;
            for (const auto &member : members_) {
                if (!first)
                    out->push_back(',');
                first = false;
                escapeInto(out, member.first);
                out->push_back(':');
                member.second.dumpTo(out);
            }
            out->push_back('}');
            break;
        }
    }
}

std::string Json::dump() const {
    std::string out;
    dumpTo(&out);
    return out;
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

namespace {

class Parser {
public:
    explicit Parser(const std::string &text) : text_(text) {}

    bool parseValue(Json *out, std::string *error) {
        skipWs();
        if (pos_ >= text_.size()) {
            *error = "unexpected end of input";
            return false;
        }
        char c = text_[pos_];
        switch (c) {
            case '{': {
                Json obj = Json::object();
                pos_++;
                skipWs();
                if (peek() == '}') {
                    pos_++;
                    *out = std::move(obj);
                    return true;
                }
                for (;;) {
                    skipWs();
                    std::string key;
                    if (!parseString(&key, error))
                        return false;
                    skipWs();
                    if (!expect(':', error))
                        return false;
                    skipWs();
                    Json value;
                    if (!parseValue(&value, error))
                        return false;
                    obj.set(key, std::move(value));
                    skipWs();
                    char sep = peek();
                    if (sep == ',') {
                        pos_++;
                        continue;
                    }
                    if (sep == '}') {
                        pos_++;
                        *out = std::move(obj);
                        return true;
                    }
                    *error = "expected ',' or '}' in object";
                    return false;
                }
            }
            case '[': {
                Json arr = Json::array();
                pos_++;
                skipWs();
                if (peek() == ']') {
                    pos_++;
                    *out = std::move(arr);
                    return true;
                }
                for (;;) {
                    skipWs();
                    Json value;
                    if (!parseValue(&value, error))
                        return false;
                    arr.push(std::move(value));
                    skipWs();
                    char sep = peek();
                    if (sep == ',') {
                        pos_++;
                        continue;
                    }
                    if (sep == ']') {
                        pos_++;
                        *out = std::move(arr);
                        return true;
                    }
                    *error = "expected ',' or ']' in array";
                    return false;
                }
            }
            case '"': {
                std::string s;
                if (!parseString(&s, error))
                    return false;
                *out = Json::string(std::move(s));
                return true;
            }
            case 't':
                return expectLiteral("true", Json::boolean(true), out, error);
            case 'f':
                return expectLiteral("false", Json::boolean(false), out, error);
            case 'n':
                return expectLiteral("null", Json(), out, error);
            default:
                if (c == '-' || (c >= '0' && c <= '9'))
                    return parseNumber(out, error);
                *error = std::string("unexpected character '") + c + "'";
                return false;
        }
    }

private:
    char peek() const { return pos_ < text_.size() ? text_[pos_] : '\0'; }

    void skipWs() {
        while (pos_ < text_.size()) {
            char c = text_[pos_];
            if (c == ' ' || c == '\t' || c == '\n' || c == '\r')
                pos_++;
            else
                break;
        }
    }

    bool expect(char c, std::string *error) {
        if (peek() != c) {
            *error = std::string("expected '") + c + "'";
            return false;
        }
        pos_++;
        return true;
    }

    bool expectLiteral(const char *lit, Json value, Json *out,
                       std::string *error) {
        size_t n = std::strlen(lit);
        if (text_.compare(pos_, n, lit) != 0) {
            *error = std::string("invalid literal, expected '") + lit + "'";
            return false;
        }
        pos_ += n;
        *out = std::move(value);
        return true;
    }

    // \uXXXX plus UTF-8 byte passthrough; handles surrogate pairs.
    bool parseString(std::string *out, std::string *error) {
        if (!expect('"', error))
            return false;
        while (pos_ < text_.size()) {
            unsigned char c = static_cast<unsigned char>(text_[pos_]);
            if (c == '"') {
                pos_++;
                return true;
            }
            if (c == '\\') {
                pos_++;
                if (pos_ >= text_.size())
                    break;
                char esc = text_[pos_++];
                switch (esc) {
                    case '"':
                        out->push_back('"');
                        break;
                    case '\\':
                        out->push_back('\\');
                        break;
                    case '/':
                        out->push_back('/');
                        break;
                    case 'b':
                        out->push_back('\b');
                        break;
                    case 'f':
                        out->push_back('\f');
                        break;
                    case 'n':
                        out->push_back('\n');
                        break;
                    case 'r':
                        out->push_back('\r');
                        break;
                    case 't':
                        out->push_back('\t');
                        break;
                    case 'u': {
                        uint32_t cp = 0;
                        if (!parseHex4(&cp, error))
                            return false;
                        // UTF-16 surrogate pair?
                        if (cp >= 0xD800 && cp <= 0xDBFF) {
                            if (pos_ + 1 < text_.size() && text_[pos_] == '\\' &&
                                text_[pos_ + 1] == 'u') {
                                pos_ += 2;
                                uint32_t low = 0;
                                if (!parseHex4(&low, error))
                                    return false;
                                if (low >= 0xDC00 && low <= 0xDFFF) {
                                    cp = 0x10000 + ((cp - 0xD800) << 10) +
                                         (low - 0xDC00);
                                } else {
                                    *error = "invalid low surrogate";
                                    return false;
                                }
                            } else {
                                *error = "unpaired high surrogate";
                                return false;
                            }
                        } else if (cp >= 0xDC00 && cp <= 0xDFFF) {
                            *error = "unpaired low surrogate";
                            return false;
                        }
                        appendUtf8(cp, out);
                        break;
                    }
                    default:
                        *error = "invalid escape sequence";
                        return false;
                }
                continue;
            }
            // Raw UTF-8 continuation bytes pass through untouched.
            out->push_back(static_cast<char>(c));
            pos_++;
        }
        *error = "unterminated string";
        return false;
    }

    bool parseHex4(uint32_t *out, std::string *error) {
        if (pos_ + 4 > text_.size()) {
            *error = "truncated \\u escape";
            return false;
        }
        uint32_t v = 0;
        for (int i = 0; i < 4; ++i) {
            char c = text_[pos_ + i];
            int d;
            if (c >= '0' && c <= '9')
                d = c - '0';
            else if (c >= 'a' && c <= 'f')
                d = c - 'a' + 10;
            else if (c >= 'A' && c <= 'F')
                d = c - 'A' + 10;
            else {
                *error = "invalid \\u escape";
                return false;
            }
            v = (v << 4) | static_cast<uint32_t>(d);
        }
        pos_ += 4;
        *out = v;
        return true;
    }

    static void appendUtf8(uint32_t cp, std::string *out) {
        if (cp < 0x80) {
            out->push_back(static_cast<char>(cp));
        } else if (cp < 0x800) {
            out->push_back(static_cast<char>(0xC0 | (cp >> 6)));
            out->push_back(static_cast<char>(0x80 | (cp & 0x3F)));
        } else if (cp < 0x10000) {
            out->push_back(static_cast<char>(0xE0 | (cp >> 12)));
            out->push_back(static_cast<char>(0x80 | ((cp >> 6) & 0x3F)));
            out->push_back(static_cast<char>(0x80 | (cp & 0x3F)));
        } else {
            out->push_back(static_cast<char>(0xF0 | (cp >> 18)));
            out->push_back(static_cast<char>(0x80 | ((cp >> 12) & 0x3F)));
            out->push_back(static_cast<char>(0x80 | ((cp >> 6) & 0x3F)));
            out->push_back(static_cast<char>(0x80 | (cp & 0x3F)));
        }
    }

    bool parseNumber(Json *out, std::string *error) {
        size_t start = pos_;
        if (peek() == '-')
            pos_++;
        bool hasDigits = false;
        while (pos_ < text_.size() && text_[pos_] >= '0' && text_[pos_] <= '9') {
            pos_++;
            hasDigits = true;
        }
        if (!hasDigits) {
            *error = "invalid number";
            return false;
        }
        if (pos_ < text_.size() && text_[pos_] == '.') {
            pos_++;
            while (pos_ < text_.size() && text_[pos_] >= '0' &&
                   text_[pos_] <= '9')
                pos_++;
        }
        if (pos_ < text_.size() &&
            (text_[pos_] == 'e' || text_[pos_] == 'E')) {
            pos_++;
            if (pos_ < text_.size() &&
                (text_[pos_] == '+' || text_[pos_] == '-'))
                pos_++;
            bool expDigits = false;
            while (pos_ < text_.size() && text_[pos_] >= '0' &&
                   text_[pos_] <= '9') {
                pos_++;
                expDigits = true;
            }
            if (!expDigits) {
                *error = "invalid number exponent";
                return false;
            }
        }
        std::string num = text_.substr(start, pos_ - start);
        char *end = nullptr;
        errno = 0;
        double d = std::strtod(num.c_str(), &end);
        if (errno == ERANGE || end == nullptr || *end != '\0') {
            *error = "number out of range";
            return false;
        }
        *out = Json::number(d);
        return true;
    }

    const std::string &text_;
    size_t pos_ = 0;
};

}  // namespace

bool Json::parse(const std::string &text, Json *out, std::string *error) {
    Parser parser(text);
    std::string err;
    if (!parser.parseValue(out, &err)) {
        if (error)
            *error = err;
        return false;
    }
    return true;
}

}  // namespace pskk
