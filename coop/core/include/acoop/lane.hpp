// Reliable, ordered message lane to one remote peer, over an unreliable datagram transport.
// Every reliable message gets a sequence number; the receiver delivers each exactly once and in order, and
// acknowledges cumulatively. ACU sends intentions through Storm with the same guarantees (ACU-RE-0082).
#pragma once
#include <cstdint>
#include <deque>
#include <map>
#include <vector>

#include "wire.hpp"

namespace acoop {

// Packet header: 'A' 'C', version, kind. Then:
//   kind 0 (unreliable): body
//   kind 1 (reliable):   u32 seq, u32 cumulative ack, body
//   kind 2 (ack):        u32 cumulative ack
// A body is u8 message type + payload (session.hpp).
constexpr std::uint8_t kMagic0 = 'A', kMagic1 = 'C', kVersion = 1;
enum class PacketKind : std::uint8_t { Unreliable = 0, Reliable = 1, Ack = 2 };

struct LaneConfig {
    std::uint32_t resend_ms = 200;        // resend an unacknowledged message after this long
    std::size_t max_pending = 4096;       // outgoing messages kept for resend
    std::size_t max_early = 1024;         // out-of-order messages buffered on receive
};

class Lane {
public:
    Lane() = default;
    explicit Lane(LaneConfig cfg) : cfg_(cfg) {}

    // Queue a reliable body. Returns false (and drops it) if the resend window is full.
    bool push(std::vector<std::uint8_t> body, std::uint64_t now_ms, std::vector<std::vector<std::uint8_t>>& out) {
        if (pending_.size() >= cfg_.max_pending) { ++overflow_; return false; }
        pending_.push_back({next_seq_++, std::move(body), now_ms});
        out.push_back(frame(pending_.back()));
        return true;
    }

    static std::vector<std::uint8_t> unreliable(const std::vector<std::uint8_t>& body) {
        Writer w;
        header(w, PacketKind::Unreliable);
        w.bytes(body.data(), body.size());
        return w.data();
    }

    // Resend what has waited longer than resend_ms.
    void tick(std::uint64_t now_ms, std::vector<std::vector<std::uint8_t>>& out) {
        for (auto& p : pending_) {
            if (now_ms - p.sent_ms >= cfg_.resend_ms) {
                p.sent_ms = now_ms;
                out.push_back(frame(p));
                ++resent_;
            }
        }
        if (ack_due_) {
            Writer w;
            header(w, PacketKind::Ack);
            w.u32(delivered_);
            out.push_back(w.data());
            ack_due_ = false;
        }
    }

    // Feed one received packet. Bodies ready for delivery (in order) are appended to `deliver`.
    // Returns false for a malformed packet.
    bool receive(const std::uint8_t* p, std::size_t n, std::vector<std::vector<std::uint8_t>>& deliver) {
        Reader r(p, n);
        if (r.u8() != kMagic0 || r.u8() != kMagic1 || r.u8() != kVersion || !r.ok()) return false;
        const auto kind = static_cast<PacketKind>(r.u8());
        if (!r.ok()) return false;
        if (kind == PacketKind::Unreliable) {
            deliver.emplace_back(r.cursor(), r.cursor() + r.remaining());
            return true;
        }
        if (kind == PacketKind::Ack) {
            const std::uint32_t ack = r.u32();
            if (!r.ok()) return false;
            on_ack(ack);
            return true;
        }
        if (kind != PacketKind::Reliable) return false;
        const std::uint32_t seq = r.u32();
        const std::uint32_t ack = r.u32();
        if (!r.ok() || seq == 0) return false;
        on_ack(ack);
        ack_due_ = true;
        if (seq <= delivered_) { ++duplicates_; return true; }
        if (seq == delivered_ + 1) {
            deliver.emplace_back(r.cursor(), r.cursor() + r.remaining());
            ++delivered_;
            for (auto it = early_.find(delivered_ + 1); it != early_.end(); it = early_.find(delivered_ + 1)) {
                deliver.push_back(std::move(it->second));
                early_.erase(it);
                ++delivered_;
            }
            return true;
        }
        if (early_.size() < cfg_.max_early) early_.emplace(seq, std::vector<std::uint8_t>(r.cursor(), r.cursor() + r.remaining()));
        return true;
    }

    std::size_t pending() const { return pending_.size(); }
    std::uint64_t resent() const { return resent_; }
    std::uint64_t duplicates() const { return duplicates_; }
    std::uint64_t overflow() const { return overflow_; }

private:
    struct Pending {
        std::uint32_t seq;
        std::vector<std::uint8_t> body;
        std::uint64_t sent_ms;
    };
    static void header(Writer& w, PacketKind k) {
        w.u8(kMagic0);
        w.u8(kMagic1);
        w.u8(kVersion);
        w.u8(static_cast<std::uint8_t>(k));
    }
    std::vector<std::uint8_t> frame(const Pending& p) const {
        Writer w;
        header(w, PacketKind::Reliable);
        w.u32(p.seq);
        w.u32(delivered_);
        w.bytes(p.body.data(), p.body.size());
        return w.data();
    }
    void on_ack(std::uint32_t ack) {
        while (!pending_.empty() && pending_.front().seq <= ack) pending_.pop_front();
    }

    LaneConfig cfg_;
    std::uint32_t next_seq_ = 1;
    std::deque<Pending> pending_;
    std::uint32_t delivered_ = 0;
    std::map<std::uint32_t, std::vector<std::uint8_t>> early_;
    bool ack_due_ = false;
    std::uint64_t resent_ = 0, duplicates_ = 0, overflow_ = 0;
};

}  // namespace acoop
