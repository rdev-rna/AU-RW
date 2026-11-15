# acoop core

A portable co-op core for Anvil-engine games, modelled on how Assassin's Creed Unity runs co-op.
Header-only C++17 with no dependencies beyond the standard library. It builds with MSVC (x86 and x64, `/W4 /WX`)
and with GCC (`-Wall -Wextra -Wpedantic -Wconversion -Werror`).

The core holds the rules ACU uses. It knows nothing about a particular game:

| Rule | What the core does | ACU evidence |
|---|---|---|
| One owner per entity | Each shared entity has one master machine. Every other machine holds a replica. | role bits `entity+212` (ACU-RE-0092 obs 11, 17; AVATAR_SPEC R1, D10) |
| Local creation makes a master | A machine that loads an unknown entity takes it and announces it. If two machines load it at once, the lower peer id wins. | `sub_140C1E360` local vs data-driven creator (obs 17) |
| Ownership migrates with hysteresis | A machine claims an entity when its player is inside `claim_m` (default 50 m) and the owner's player is outside `release_m` (default 57 m). The owner decides and broadcasts a grant. An owner that unloads the entity hands it to the nearest other player. | obs 18–20, AVATAR_SPEC D11–D12 |
| Damage is a request to the owner | A hit on a replica becomes a request to the owner. Only the owner applies it and decides death, then broadcasts the outcome. A request that arrives at a former owner is forwarded to the new one. | obs 10, 14, 15; D1–D9 |
| Replicas display the owner's life | Replicas never keep a health pool of their own; they show the owner's value. Death happens once per machine. | `RFCSetMasterLife` (obs 11, 13, 16) |
| Join resync | When a peer connects, every machine re-sends the entities it owns to that peer. | ACU-RE-0083 |
| Reliable lane | Ordered, exactly-once messages with cumulative acks, over any datagram transport. | intention flush (ACU-RE-0082) |

The defaults `claim_m = 50` and `release_m = 57` are an implementation choice inside the measured bounds: claim
between 49.83 and 52.56 m, release between 55.48 and 88.8 m (between 55.48 and 59 m if NPCs follow the same rule as
the alarm bell). They come from one alarm bell and Danton in one mission. Tune them per entity type once more is
measured.

## Plugging a game in

A mod implements two small interfaces and drives a `Session` from its game thread:

```cpp
#include <acoop/acoop.hpp>

struct MyGame : acoop::GameAdapter {
    bool entity_position(acoop::NetId id, acoop::Vec3& out) override;           // false if not loaded here
    void on_role_changed(acoop::NetId id, acoop::Role r, const acoop::EntityState& s) override;
        // Master: switch the entity's AI on and adopt s.life. Replica: switch its AI off; drive it from updates.
    std::int32_t apply_damage(acoop::NetId id, std::int32_t amount, acoop::NetId attacker) override;
        // master only: the game's own damage rules; return the new life
    void show_life(acoop::NetId id, std::int32_t life) override;                 // replica: display the owner's value
    void on_death(acoop::NetId id) override;                                      // once per entity, every machine
};
struct MyNet : acoop::Transport {
    void send(acoop::PeerId to, const std::uint8_t* data, std::size_t size) override;  // one UDP datagram
};

acoop::Session session(my_peer_id, net, game);
session.connect_peer(other);                       // once the transport can reach it
// each frame:
session.set_player_position(player_pos);
session.tick(now_ms);
// on game events:
session.entity_loaded(id);   session.entity_unloaded(id);
session.local_hit(victim_id, damage, attacker_id); // instead of applying damage to a replica
// on every received datagram:
session.on_packet(from, data, size);
```

The adapter decides what an entity's `NetId` is. It must be the same value on both machines and must never be a
pointer. For placed objects, use the data id both machines load. For spawned soldiers, use something both sides can
compute, such as the spawner's data id plus the spawn index.

Movement, animation and AI intentions (ACU's `NetHumanIntention` / `NetBehaviorAI` streams) are not in this
first slice. Use the reliable lane for intentions; send poses unreliably.

## Building the tests

```
build_msvc.cmd <output-dir>          :: MSVC x86 + x64
g++ -std=c++17 -Wall -Wextra -Wpedantic -Wconversion -Werror -Iinclude tests/test_session.cpp -o test.exe
```

`tests/test_session.cpp` runs every rule over an in-memory network with fake games (75 checks). The hand-over test
replays the live ACU runs (`migrate1/2/3.jsonl`, ACU-RE-0092 obs 19–20) step by step, and the default policy gives
the same owner at every measured step. Other tests cover:
- announcement races, including an entity loaded before its peer connected;
- simultaneous lethal hits, which give one death;
- damage sent while a hand-over is in flight, forwarded and applied once;
- a peer that joins late;
- an owner that disconnects;
- 60 s of random walking under 20 % packet loss, with no tick ever showing two masters.

These are synthetic tests. They show that the library implements the measured rule, not that a game behaves
correctly with it.

## Not yet covered

- Pose and intention streams, and the relevance filter that decides which replicas exist on a machine.
- Per-entity-type distances, and whether combat should block a hand-over.
- Encryption or authentication of packets. The lane rejects malformed packets but trusts its peers.
- State transfer beyond life and death (alert level, current target) when ownership moves.
