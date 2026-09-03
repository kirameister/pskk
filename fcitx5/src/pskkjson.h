/*
 * SPDX-License-Identifier: MIT
 *
 * pskkjson.h - Minimal dependency-free JSON DOM used by the PSKK Fcitx 5
 * addon to speak the pskk-server JSON protocol. Implements only the subset
 * of JSON the protocol needs. All strings are UTF-8.
 */
#ifndef PSKK_FCITX5_PSKKJSON_H_
#define PSKK_FCITX5_PSKKJSON_H_

#include <cstddef>
#include <cstdint>
#include <string>
#include <utility>
#include <vector>

namespace pskk {

class Json {
public:
    enum class Type { Null, Boolean, Number, String, Array, Object };

    Json() : type_(Type::Null) {}
    static Json boolean(bool b);
    static Json number(double n);
    static Json string(std::string s);
    static Json array();
    static Json object();

    Type type() const { return type_; }
    bool isNull() const { return type_ == Type::Null; }
    bool isBool() const { return type_ == Type::Boolean; }
    bool isNumber() const { return type_ == Type::Number; }
    bool isString() const { return type_ == Type::String; }
    bool isArray() const { return type_ == Type::Array; }
    bool isObject() const { return type_ == Type::Object; }

    bool asBool() const { return boolValue_; }
    double asNumber() const { return numberValue_; }
    int64_t asInt() const { return static_cast<int64_t>(numberValue_); }
    const std::string &asString() const { return stringValue_; }

    size_t size() const {
        if (type_ == Type::Array)
            return items_.size();
        if (type_ == Type::Object)
            return members_.size();
        return 0;
    }
    const Json &at(size_t i) const { return items_.at(i); }

    /// Object member lookup; returns nullptr when missing.
    const Json *find(const std::string &key) const;

    /// Object builder: set(key, value), returns *this.
    Json &set(const std::string &key, Json value);
    /// Array builder: append value, returns *this.
    Json &push(Json value);

    /// Serialize to a compact JSON string.
    std::string dump() const;

    /// Parse `text` into `out`. Returns false and fills `error` on failure.
    static bool parse(const std::string &text, Json *out, std::string *error);

private:
    explicit Json(Type t) : type_(t) {}

    void dumpTo(std::string *out) const;

    Type type_;
    bool boolValue_ = false;
    double numberValue_ = 0;
    std::string stringValue_;
    std::vector<Json> items_;
    std::vector<std::pair<std::string, Json>> members_;
};

}  // namespace pskk

#endif  // PSKK_FCITX5_PSKKJSON_H_
