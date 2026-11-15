// acoop: portable co-op core for Anvil-engine games, modelled on Assassin's Creed Unity's co-op.
// Engine-neutral. A game plugs in through GameAdapter, the network through Transport (session.hpp).
// Evidence for each rule: AU-RW docs/coop/AVATAR_SPEC.md and finding ACU-RE-0092.
#pragma once
#include <cmath>
#include <cstddef>
#include <cstdint>

namespace acoop {

// Peer index inside one session. 0 is the session host by convention; the core gives it no special
// authority except as the tie-break for simultaneous announcements (lower id wins).
using PeerId = std::uint8_t;
constexpr PeerId kNoPeer = 0xFF;
constexpr std::size_t kMaxPeers = 8;

// Semantic entity id, chosen by the game adapter so that both machines compute the same value for the
// same entity (for example a placed object's data id, or hash(archetype, spawn cell, spawn index)).
// Never a pointer or a per-process handle. 0 is invalid.
using NetId = std::uint64_t;

// Who simulates an entity on this machine.
//   Master  - this machine runs the entity (AI, physics, health) and sends its outcomes.
//   Replica - another machine runs it; this machine shows it and turns requests into messages.
//   Unowned - nobody has claimed it yet.
// ACU: role bits at entity+212, 0x40000 master / 0x80000 replica (ACU-RE-0092 obs 11, 17).
enum class Role : std::uint8_t { Unowned = 0, Master = 1, Replica = 2 };

struct Vec3 {
    float x = 0, y = 0, z = 0;
};

// Planar distance. The ACU hand-over distances were measured in the horizontal plane (ACU-RE-0092 obs 19-20).
inline float planar_distance(const Vec3& a, const Vec3& b) {
    const float dx = a.x - b.x, dy = a.y - b.y;
    return std::sqrt(dx * dx + dy * dy);
}

}  // namespace acoop
