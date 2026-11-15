// Bounds-checked little-endian encoding. A Reader that runs past the end sets ok() = false and returns zeros,
// so a truncated or hostile packet can never read outside its buffer.
#pragma once
#include <cstdint>
#include <cstring>
#include <vector>

#include "types.hpp"

namespace acoop {

class Writer {
public:
    void u8(std::uint8_t v) { buf_.push_back(v); }
    void u16(std::uint16_t v) { put(v, 2); }
    void u32(std::uint32_t v) { put(v, 4); }
    void u64(std::uint64_t v) { put(v, 8); }
    void i32(std::int32_t v) { u32(static_cast<std::uint32_t>(v)); }
    void f32(float v) {
        std::uint32_t bits;
        std::memcpy(&bits, &v, 4);
        u32(bits);
    }
    void vec3(const Vec3& v) { f32(v.x); f32(v.y); f32(v.z); }
    void bytes(const std::uint8_t* p, std::size_t n) { buf_.insert(buf_.end(), p, p + n); }
    const std::vector<std::uint8_t>& data() const { return buf_; }
    std::vector<std::uint8_t>& data() { return buf_; }

private:
    void put(std::uint64_t v, int n) {
        for (int i = 0; i < n; ++i) buf_.push_back(static_cast<std::uint8_t>(v >> (8 * i)));
    }
    std::vector<std::uint8_t> buf_;
};

class Reader {
public:
    Reader(const std::uint8_t* p, std::size_t n) : p_(p), n_(n) {}
    std::uint8_t u8() { return static_cast<std::uint8_t>(get(1)); }
    std::uint16_t u16() { return static_cast<std::uint16_t>(get(2)); }
    std::uint32_t u32() { return static_cast<std::uint32_t>(get(4)); }
    std::uint64_t u64() { return get(8); }
    std::int32_t i32() { return static_cast<std::int32_t>(u32()); }
    float f32() {
        const std::uint32_t bits = u32();
        float v;
        std::memcpy(&v, &bits, 4);
        return v;
    }
    Vec3 vec3() {
        Vec3 v;
        v.x = f32();
        v.y = f32();
        v.z = f32();
        return v;
    }
    bool ok() const { return ok_; }
    std::size_t remaining() const { return ok_ ? n_ - at_ : 0; }
    const std::uint8_t* cursor() const { return p_ + at_; }
    void skip(std::size_t n) {
        if (!ok_ || n > n_ - at_) { ok_ = false; return; }
        at_ += n;
    }

private:
    std::uint64_t get(std::size_t n) {
        if (!ok_ || n > n_ - at_) { ok_ = false; return 0; }
        std::uint64_t v = 0;
        for (std::size_t i = 0; i < n; ++i) v |= static_cast<std::uint64_t>(p_[at_ + i]) << (8 * i);
        at_ += n;
        return v;
    }
    const std::uint8_t* p_;
    std::size_t n_;
    std::size_t at_ = 0;
    bool ok_ = true;
};

}  // namespace acoop
