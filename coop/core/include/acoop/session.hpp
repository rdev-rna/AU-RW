// Session: per-entity ownership, hand-over and damage authority for one machine in a co-op session.
//
// The model is ACU's (AU-RW docs/coop/AVATAR_SPEC.md, finding ACU-RE-0092):
//  * Each shared entity has exactly one owner machine, which simulates it. Others hold replicas.   (R1, D10)
//  * Ownership moves with hysteresis: a machine claims an entity when its player is inside claim_m and the
//    owner's player is outside release_m; the owner arbitrates and grants. An owner that unloads the
//    entity hands it to the nearest other player.                                                 (D11, D12)
//  * Damage is a request to the victim's owner; only the owner applies it and decides death, then
//    sends the outcome. Replicas display the owner's life and never keep their own pool.          (D1-D6, D8-D9)
//  * A peer that joins receives every owned entity's state from its owner.                         (ACU-RE-0083)
// The game side implements GameAdapter; the network side implements Transport. Neither is engine-specific.
#pragma once
#include <array>
#include <cstdint>
#include <unordered_map>
#include <vector>

#include "lane.hpp"
#include "types.hpp"
#include "wire.hpp"

namespace acoop {

struct EntityState {
    std::int32_t life = 0;
    bool life_known = false;
    bool dead = false;
};

// Implemented by the game mod. Called on the game thread from Session methods.
struct GameAdapter {
    virtual ~GameAdapter() = default;
    // Position of a loaded entity. Return false if it is not loaded on this machine.
    virtual bool entity_position(NetId id, Vec3& out) = 0;
    // Role changed for a loaded entity. Master: run its AI/physics and adopt `state`.
    // Replica: stop its AI and drive it from the owner's updates.     (ACU: 0x141C62F30 / 0x141C631D0 notify components)
    virtual void on_role_changed(NetId id, Role role, const EntityState& state) = 0;
    // Master only: apply damage with the game's own rules and return the new life.
    virtual std::int32_t apply_damage(NetId id, std::int32_t amount, NetId attacker) = 0;
    // Replica only: show the owner's life value.                       (ACU: RFCSetMasterLife -> life setter)
    virtual void show_life(NetId id, std::int32_t life) = 0;
    // Called exactly once per entity on every machine when its owner decides it died.
    virtual void on_death(NetId id) = 0;
};

// Implemented by the mod's network layer: send one datagram to a peer. Delivery may fail or reorder.
struct Transport {
    virtual ~Transport() = default;
    virtual void send(PeerId to, const std::uint8_t* data, std::size_t size) = 0;
};

struct HandoverPolicy {
    // Implementation choice inside the ACU-measured bounds for one alarm bell (ACU-RE-0092 obs 19-20):
    // claim between 49.83 and 52.56 m; release between 55.48 and 88.8 m (55.48-59 m if Danton follows it).
    float claim_m = 50.0f;
    float release_m = 57.0f;
    std::uint32_t claim_retry_ms = 1000;  // after a refused or lost claim
};

struct SessionConfig {
    HandoverPolicy handover;
    LaneConfig lane;
    std::uint32_t position_interval_ms = 100;  // player position broadcast (unreliable)
};

struct SessionStats {
    std::uint64_t announces = 0, claims_sent = 0, claims_refused = 0, grants_sent = 0, grants_applied = 0,
                  grants_stale = 0, damage_requests = 0, damage_forwarded = 0, damage_dropped = 0,
                  outcomes_sent = 0, outcomes_applied = 0, outcomes_stale = 0, deaths = 0, bad_packets = 0;
};

enum class Msg : std::uint8_t {
    Hello = 1,       // reliable; receiver re-sends what it owns to the sender
    PlayerPos = 2,   // unreliable; vec3
    Announce = 3,    // reliable; id - "I loaded this, nobody owns it as far as I know, I take it" (epoch 0)
    Claim = 4,       // reliable to owner; id, epoch seen, claimant player position
    Grant = 5,       // reliable broadcast; id, epoch, owner, state
    DamageReq = 6,   // reliable to owner; id, amount, attacker, hops
    Outcome = 7,     // reliable broadcast; id, epoch, state
};

class Session {
public:
    Session(PeerId self, Transport& net, GameAdapter& game, SessionConfig cfg = {})
        : self_(self), net_(net), game_(game), cfg_(cfg) {
        for (auto& p : peers_) p.lane = Lane(cfg_.lane);
    }

    PeerId self() const { return self_; }
    const SessionStats& stats() const { return stats_; }

    // ---- peers --------------------------------------------------------------------------------------
    void connect_peer(PeerId p) {
        if (p >= kMaxPeers || p == self_) return;
        peers_[p].connected = true;
        Writer w;
        w.u8(static_cast<std::uint8_t>(Msg::Hello));
        reliable(p, w.data());
    }
    void disconnect_peer(PeerId p) {
        if (p >= kMaxPeers) return;
        peers_[p] = Peer{};
        peers_[p].lane = Lane(cfg_.lane);
        // Entities the peer owned stay assigned to it until a loaded machine claims them (the claim rule treats
        // an unknown owner position as "outside release"), so a drop never creates two masters.
    }

    // ---- local game events ----------------------------------------------------------------------------
    void set_player_position(const Vec3& v) { my_pos_ = v; have_my_pos_ = true; }

    // The entity was created or streamed in on this machine.
    void entity_loaded(NetId id) {
        if (id == 0) return;
        Rec& r = recs_[id];
        r.loaded = true;
        if (r.owner == kNoPeer) {
            // ACU: the local creator makes a master (sub_140C1E360 with a3 != 0). Tell everyone; an
            // equal-epoch race is broken by the lower peer id on every machine.
            r.owner = self_;
            r.epoch = 0;
            ++stats_.announces;
            Writer w;
            w.u8(static_cast<std::uint8_t>(Msg::Announce));
            w.u64(id);
            broadcast(w.data());
        } else if (r.owner != self_ && peers_[r.owner].connected) {
            // Ask the owner straight away: an owner that does not have the entity loaded (it was handed over on
            // unload) grants unconditionally, otherwise the normal distance rule decides.
            send_claim(id, r);
        }
        game_.on_role_changed(id, role_of(r), r.state);
    }

    // The entity was unloaded on this machine. An owner hands it to the other player nearest to it.
    void entity_unloaded(NetId id) {
        auto it = recs_.find(id);
        if (it == recs_.end()) return;
        Rec& r = it->second;
        Vec3 where{};
        bool have_where = game_.entity_position(id, where);
        if (!have_where && r.have_last_pos) { where = r.last_pos; have_where = true; }
        r.loaded = false;
        if (r.owner != self_ || !have_where) return;
        PeerId best = kNoPeer;
        float best_d = 0;
        for (PeerId p = 0; p < kMaxPeers; ++p) {
            if (!peers_[p].connected || !peers_[p].have_pos) continue;
            const float d = planar_distance(peers_[p].pos, where);
            if (best == kNoPeer || d < best_d) { best = p; best_d = d; }
        }
        if (best != kNoPeer) transfer(id, r, best);
    }

    // The local player hit an entity. Master: apply now. Replica: request it from the owner.
    void local_hit(NetId id, std::int32_t amount, NetId attacker) {
        auto it = recs_.find(id);
        if (it == recs_.end() || it->second.state.dead || amount <= 0) { ++stats_.damage_dropped; return; }
        Rec& r = it->second;
        if (r.owner == self_ && r.loaded) { apply_as_master(id, r, amount, attacker); return; }
        if (r.owner == kNoPeer || r.owner == self_) { ++stats_.damage_dropped; return; }
        send_damage(r.owner, id, amount, attacker, 0);
    }

    // Master only: the entity's life changed for another reason (regeneration, scripted heal, ...).
    void report_life(NetId id, std::int32_t life) {
        auto it = recs_.find(id);
        if (it == recs_.end() || it->second.owner != self_ || it->second.state.dead) return;
        Rec& r = it->second;
        r.state.life = life;
        r.state.life_known = true;
        if (life <= 0) mark_dead(id, r);
        send_outcome(id, r);
    }

    Role role(NetId id) const {
        auto it = recs_.find(id);
        return it == recs_.end() ? Role::Unowned : role_of(it->second);
    }
    PeerId owner(NetId id) const {
        auto it = recs_.find(id);
        return it == recs_.end() ? kNoPeer : it->second.owner;
    }
    std::uint32_t epoch(NetId id) const {
        auto it = recs_.find(id);
        return it == recs_.end() ? 0 : it->second.epoch;
    }
    EntityState state(NetId id) const {
        auto it = recs_.find(id);
        return it == recs_.end() ? EntityState{} : it->second.state;
    }

    // ---- network ---------------------------------------------------------------------------------------
    void on_packet(PeerId from, const std::uint8_t* data, std::size_t size) {
        if (from >= kMaxPeers || from == self_ || !peers_[from].connected) { ++stats_.bad_packets; return; }
        std::vector<std::vector<std::uint8_t>> bodies;
        if (!peers_[from].lane.receive(data, size, bodies)) { ++stats_.bad_packets; return; }
        for (auto& b : bodies) handle(from, b);
    }

    void tick(std::uint64_t now_ms) {
        now_ms_ = now_ms;
        if (have_my_pos_ && now_ms - last_pos_ms_ >= cfg_.position_interval_ms) {
            last_pos_ms_ = now_ms;
            Writer w;
            w.u8(static_cast<std::uint8_t>(Msg::PlayerPos));
            w.vec3(my_pos_);
            const auto pkt = Lane::unreliable(w.data());
            for (PeerId p = 0; p < kMaxPeers; ++p)
                if (peers_[p].connected) net_.send(p, pkt.data(), pkt.size());
        }
        evaluate_claims();
        std::vector<std::vector<std::uint8_t>> out;
        for (PeerId p = 0; p < kMaxPeers; ++p) {
            if (!peers_[p].connected) continue;
            out.clear();
            peers_[p].lane.tick(now_ms, out);
            for (auto& pkt : out) net_.send(p, pkt.data(), pkt.size());
        }
    }

private:
    struct Rec {
        PeerId owner = kNoPeer;
        std::uint32_t epoch = 0;
        bool loaded = false;
        EntityState state;
        Vec3 last_pos{};
        bool have_last_pos = false;
        std::uint64_t last_claim_ms = 0;
        bool claim_pending = false;
        std::uint32_t claim_epoch = 0;
    };
    struct Peer {
        bool connected = false;
        bool have_pos = false;
        Vec3 pos{};
        Lane lane;
    };

    Role role_of(const Rec& r) const {
        if (r.owner == kNoPeer) return Role::Unowned;
        return r.owner == self_ ? Role::Master : Role::Replica;
    }

    void reliable(PeerId p, const std::vector<std::uint8_t>& body) {
        std::vector<std::vector<std::uint8_t>> out;
        peers_[p].lane.push(body, now_ms_, out);
        for (auto& pkt : out) net_.send(p, pkt.data(), pkt.size());
    }
    void broadcast(const std::vector<std::uint8_t>& body) {
        for (PeerId p = 0; p < kMaxPeers; ++p)
            if (peers_[p].connected) reliable(p, body);
    }

    void write_grant(Writer& w, NetId id, const Rec& r) const {
        w.u8(static_cast<std::uint8_t>(Msg::Grant));
        w.u64(id);
        w.u32(r.epoch);
        w.u8(r.owner);
        write_state(w, r.state);
    }
    static void write_state(Writer& w, const EntityState& s) {
        w.i32(s.life);
        w.u8(static_cast<std::uint8_t>((s.life_known ? 1 : 0) | (s.dead ? 2 : 0)));
    }
    static EntityState read_state(Reader& rd) {
        EntityState s;
        s.life = rd.i32();
        const std::uint8_t f = rd.u8();
        s.life_known = (f & 1) != 0;
        s.dead = (f & 2) != 0;
        return s;
    }

    // Owner gives the entity to `to`. The old owner becomes a replica before the grant leaves, so two
    // masters never exist; between send and receipt there is briefly no master.
    void transfer(NetId id, Rec& r, PeerId to) {
        r.owner = to;
        ++r.epoch;
        ++stats_.grants_sent;
        if (r.loaded) game_.on_role_changed(id, Role::Replica, r.state);
        Writer w;
        write_grant(w, id, r);
        broadcast(w.data());
    }

    void evaluate_claims() {
        for (auto& kv : recs_) {
            Rec& r = kv.second;
            if (!r.loaded) continue;
            Vec3 at;
            if (!game_.entity_position(kv.first, at)) continue;
            r.last_pos = at;
            r.have_last_pos = true;
            if (r.state.dead || r.owner == self_ || r.owner == kNoPeer) continue;
            const Peer& o = peers_[r.owner];
            if (!o.connected) {
                // The owner left. Every machine that has the entity loaded takes over at the next epoch; equal
                // epochs are broken by the lower peer id in on_grant, so all machines settle on one master.
                r.owner = self_;
                ++r.epoch;
                ++stats_.grants_sent;
                game_.on_role_changed(kv.first, Role::Master, r.state);
                Writer w;
                write_grant(w, kv.first, r);
                broadcast(w.data());
                continue;
            }
            if (!have_my_pos_ || planar_distance(my_pos_, at) >= cfg_.handover.claim_m) continue;
            // Unknown owner position counts as outside the release distance.
            if (o.have_pos && planar_distance(o.pos, at) <= cfg_.handover.release_m) continue;
            if (r.claim_pending && r.claim_epoch == r.epoch && now_ms_ - r.last_claim_ms < cfg_.handover.claim_retry_ms)
                continue;
            send_claim(kv.first, r);
        }
    }

    void send_claim(NetId id, Rec& r) {
        r.claim_pending = true;
        r.claim_epoch = r.epoch;
        r.last_claim_ms = now_ms_;
        ++stats_.claims_sent;
        Writer w;
        w.u8(static_cast<std::uint8_t>(Msg::Claim));
        w.u64(id);
        w.u32(r.epoch);
        w.u8(have_my_pos_ ? 1 : 0);
        w.vec3(my_pos_);
        reliable(r.owner, w.data());
    }

    void apply_as_master(NetId id, Rec& r, std::int32_t amount, NetId attacker) {
        r.state.life = game_.apply_damage(id, amount, attacker);
        r.state.life_known = true;
        if (r.state.life <= 0) mark_dead(id, r);
        send_outcome(id, r);
    }
    void mark_dead(NetId id, Rec& r) {
        if (r.state.dead) return;
        r.state.dead = true;
        ++stats_.deaths;
        game_.on_death(id);
    }
    void send_outcome(NetId id, const Rec& r) {
        ++stats_.outcomes_sent;
        Writer w;
        w.u8(static_cast<std::uint8_t>(Msg::Outcome));
        w.u64(id);
        w.u32(r.epoch);
        write_state(w, r.state);
        broadcast(w.data());
    }
    void send_damage(PeerId to, NetId id, std::int32_t amount, NetId attacker, std::uint8_t hops) {
        ++stats_.damage_requests;
        Writer w;
        w.u8(static_cast<std::uint8_t>(Msg::DamageReq));
        w.u64(id);
        w.i32(amount);
        w.u64(attacker);
        w.u8(hops);
        reliable(to, w.data());
    }

    void handle(PeerId from, const std::vector<std::uint8_t>& body) {
        Reader rd(body.data(), body.size());
        const auto type = static_cast<Msg>(rd.u8());
        switch (type) {
        case Msg::Hello: {
            if (!rd.ok()) break;
            // Join resync: re-send everything this machine owns to the newcomer (ACU-RE-0083).
            for (auto& kv : recs_) {
                if (kv.second.owner != self_) continue;
                Writer w;
                write_grant(w, kv.first, kv.second);
                reliable(from, w.data());
            }
            return;
        }
        case Msg::PlayerPos: {
            const Vec3 v = rd.vec3();
            if (!rd.ok()) break;
            peers_[from].pos = v;
            peers_[from].have_pos = true;
            return;
        }
        case Msg::Announce: {
            const NetId id = rd.u64();
            if (!rd.ok() || id == 0) break;
            on_announce(from, id);
            return;
        }
        case Msg::Claim: {
            const NetId id = rd.u64();
            const std::uint32_t ep = rd.u32();
            const bool has_pos = rd.u8() != 0;
            const Vec3 claimant = rd.vec3();
            if (!rd.ok()) break;
            on_claim(from, id, ep, has_pos, claimant);
            return;
        }
        case Msg::Grant: {
            const NetId id = rd.u64();
            const std::uint32_t ep = rd.u32();
            const PeerId owner = rd.u8();
            const EntityState s = read_state(rd);
            if (!rd.ok() || id == 0 || (owner >= kMaxPeers)) break;
            on_grant(id, ep, owner, s);
            return;
        }
        case Msg::DamageReq: {
            const NetId id = rd.u64();
            const std::int32_t amount = rd.i32();
            const NetId attacker = rd.u64();
            const std::uint8_t hops = rd.u8();
            if (!rd.ok()) break;
            on_damage_request(id, amount, attacker, hops);
            return;
        }
        case Msg::Outcome: {
            const NetId id = rd.u64();
            const std::uint32_t ep = rd.u32();
            const EntityState s = read_state(rd);
            if (!rd.ok()) break;
            on_outcome(from, id, ep, s);
            return;
        }
        }
        ++stats_.bad_packets;
    }

    void on_announce(PeerId from, NetId id) {
        Rec& r = recs_[id];
        if (r.owner == kNoPeer) {
            r.owner = from;
            r.epoch = 0;
            if (r.loaded) game_.on_role_changed(id, Role::Replica, r.state);
            return;
        }
        if (r.epoch == 0 && r.owner == self_) {
            // Both machines loaded it first. Lower peer id wins, decided identically on both sides.
            if (from < self_) {
                r.owner = from;
                if (r.loaded) game_.on_role_changed(id, Role::Replica, r.state);
            } else {
                // We win. The announcer may never have seen our announcement (we loaded it before it connected),
                // so tell it directly; its on_grant applies the same tie-break.
                Writer w;
                write_grant(w, id, r);
                reliable(from, w.data());
            }
            return;
        }
        if (r.owner == self_) {
            // The announcer is behind: tell it the current owner and state.
            Writer w;
            write_grant(w, id, r);
            reliable(from, w.data());
        }
    }

    void on_claim(PeerId from, NetId id, std::uint32_t ep, bool has_pos, const Vec3& claimant) {
        auto it = recs_.find(id);
        if (it == recs_.end() || it->second.owner != self_ || it->second.epoch != ep || it->second.state.dead) {
            ++stats_.claims_refused;
            return;
        }
        Rec& r = it->second;
        Vec3 at;
        // An owner that does not have the entity loaded cannot simulate it: give it to whoever asks.
        if (!r.loaded || !game_.entity_position(id, at)) { transfer(id, r, from); return; }
        // The owner re-checks with its own authoritative entity position.
        const bool claimant_inside = has_pos && planar_distance(claimant, at) < cfg_.handover.claim_m;
        const bool owner_outside = !have_my_pos_ || planar_distance(my_pos_, at) > cfg_.handover.release_m;
        if (!claimant_inside || !owner_outside) { ++stats_.claims_refused; return; }
        transfer(id, r, from);
    }

    void on_grant(NetId id, std::uint32_t ep, PeerId owner, const EntityState& s) {
        Rec& r = recs_[id];
        // Higher epoch wins. Equal epochs only arise from simultaneous announcements or simultaneous take-overs
        // after an owner left; the lower peer id wins, decided identically on every machine.
        const bool newer = ep > r.epoch || (ep == r.epoch && (r.owner == kNoPeer || (owner != r.owner && owner < r.owner)));
        if (!newer) { ++stats_.grants_stale; return; }
        const Role before = role_of(r);
        r.owner = owner;
        r.epoch = ep;
        r.claim_pending = false;
        const bool was_dead = r.state.dead;
        r.state = s;
        r.state.dead = was_dead || s.dead;
        ++stats_.grants_applied;
        if (r.loaded) {
            const Role now = role_of(r);
            if (now != before) game_.on_role_changed(id, now, r.state);
            else if (now == Role::Replica && r.state.life_known) game_.show_life(id, r.state.life);
        }
        if (s.dead && !was_dead) {
            ++stats_.deaths;
            game_.on_death(id);
        }
    }

    void on_damage_request(NetId id, std::int32_t amount, NetId attacker, std::uint8_t hops) {
        auto it = recs_.find(id);
        if (it == recs_.end() || it->second.state.dead || amount <= 0) { ++stats_.damage_dropped; return; }
        Rec& r = it->second;
        if (r.owner == self_) {
            if (!r.loaded) { ++stats_.damage_dropped; return; }
            apply_as_master(id, r, amount, attacker);
            return;
        }
        // Ownership moved while the request was in flight: forward it to the owner we know (ACU route 1).
        if (r.owner == kNoPeer || hops >= 2 || !peers_[r.owner].connected) { ++stats_.damage_dropped; return; }
        ++stats_.damage_forwarded;
        send_damage(r.owner, id, amount, attacker, static_cast<std::uint8_t>(hops + 1));
    }

    void on_outcome(PeerId from, NetId id, std::uint32_t ep, const EntityState& s) {
        Rec& r = recs_[id];
        if (ep < r.epoch || r.owner == self_) { ++stats_.outcomes_stale; return; }
        if (r.owner == kNoPeer) { r.owner = from; r.epoch = ep; }  // the sender of an outcome is its owner
        const bool was_dead = r.state.dead;
        r.state = s;
        r.state.dead = was_dead || s.dead;
        ++stats_.outcomes_applied;
        if (r.loaded && role_of(r) == Role::Replica && s.life_known) game_.show_life(id, s.life);
        if (s.dead && !was_dead) {
            ++stats_.deaths;
            game_.on_death(id);
        }
    }

    PeerId self_;
    Transport& net_;
    GameAdapter& game_;
    SessionConfig cfg_;
    SessionStats stats_;
    std::array<Peer, kMaxPeers> peers_{};
    std::unordered_map<NetId, Rec> recs_;
    Vec3 my_pos_{};
    bool have_my_pos_ = false;
    std::uint64_t now_ms_ = 0, last_pos_ms_ = 0;
};

}  // namespace acoop
