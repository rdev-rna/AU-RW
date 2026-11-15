// acoop tests. Synthetic: an in-memory network and fake games. Positions and distances in the hand-over
// tests replay the live ACU runs of ACU-RE-0092 (migrate1/2/3.jsonl, observations 19-20) so the default policy
// reproduces every measured outcome. This validates the library against the measured rule, not ACU itself.
#include <acoop/acoop.hpp>

#include <cstdio>
#include <cstdlib>
#include <deque>
#include <map>
#include <memory>
#include <string>
#include <vector>

using namespace acoop;

static int g_failed = 0, g_checks = 0;
#define CHECK(cond)                                                                 \
    do {                                                                            \
        ++g_checks;                                                                 \
        if (!(cond)) {                                                              \
            ++g_failed;                                                             \
            std::printf("  FAIL %s:%d: %s\n", __FILE__, __LINE__, #cond);           \
        }                                                                           \
    } while (0)

struct Rng {
    std::uint64_t s;
    explicit Rng(std::uint64_t seed) : s(seed ? seed : 1) {}
    std::uint32_t next() {
        s ^= s << 13; s ^= s >> 7; s ^= s << 17;
        return static_cast<std::uint32_t>(s);
    }
    float unit() { return static_cast<float>(next() & 0xFFFFFF) / float(0x1000000); }
};

// ---- fake game ---------------------------------------------------------------------------------------------
struct FakeEntity {
    Vec3 pos{};
    bool loaded = false;
    Role role = Role::Unowned;
    std::int32_t life = 100;      // the game's own value (master) / displayed value (replica)
    int deaths = 0;
    int damage_applied = 0;
};

struct FakeGame : GameAdapter {
    std::map<NetId, FakeEntity> ents;
    bool entity_position(NetId id, Vec3& out) override {
        auto it = ents.find(id);
        if (it == ents.end() || !it->second.loaded) return false;
        out = it->second.pos;
        return true;
    }
    void on_role_changed(NetId id, Role role, const EntityState& s) override {
        FakeEntity& e = ents[id];
        e.role = role;
        if (s.life_known) e.life = s.life;
    }
    std::int32_t apply_damage(NetId id, std::int32_t amount, NetId) override {
        FakeEntity& e = ents[id];
        ++e.damage_applied;
        e.life -= amount;
        return e.life;
    }
    void show_life(NetId id, std::int32_t life) override { ents[id].life = life; }
    void on_death(NetId id) override { ++ents[id].deaths; }
};

// ---- in-memory network -----------------------------------------------------------------------------------
struct Packet {
    PeerId from, to;
    std::vector<std::uint8_t> data;
    std::uint64_t at;
};

struct Sim;
struct SimTransport : Transport {
    Sim* sim;
    PeerId self;
    SimTransport(Sim* s, PeerId p) : sim(s), self(p) {}
    void send(PeerId to, const std::uint8_t* d, std::size_t n) override;
};

struct Sim {
    std::vector<std::unique_ptr<FakeGame>> games;
    std::vector<std::unique_ptr<SimTransport>> nets;
    std::vector<std::unique_ptr<Session>> peers;
    std::deque<Packet> wire;
    std::uint64_t now = 0;
    float loss = 0.0f;
    std::uint32_t latency_ms = 30, jitter_ms = 0;
    Rng rng{12345};
    bool check_masters = true;
    int double_masters = 0;
    std::vector<bool> up;

    explicit Sim(int n, SessionConfig cfg = {}) {
        for (int i = 0; i < n; ++i) {
            games.emplace_back(new FakeGame);
            nets.emplace_back(new SimTransport(this, static_cast<PeerId>(i)));
            peers.emplace_back(new Session(static_cast<PeerId>(i), *nets.back(), *games.back(), cfg));
            up.push_back(true);
        }
    }
    void connect_all() {
        for (std::size_t a = 0; a < peers.size(); ++a)
            for (std::size_t b = 0; b < peers.size(); ++b)
                if (a != b) peers[a]->connect_peer(static_cast<PeerId>(b));
    }
    void enqueue(PeerId from, PeerId to, const std::uint8_t* d, std::size_t n) {
        if (loss > 0 && rng.unit() < loss) return;
        const std::uint64_t delay = latency_ms + (jitter_ms ? rng.next() % jitter_ms : 0);
        wire.push_back({from, to, std::vector<std::uint8_t>(d, d + n), now + delay});
    }
    void step(std::uint64_t dt = 10) {
        now += dt;
        for (std::size_t i = 0; i < wire.size();) {
            if (wire[i].at <= now) {
                Packet p = std::move(wire[i]);
                wire.erase(wire.begin() + static_cast<std::ptrdiff_t>(i));
                if (p.to < peers.size() && up[p.to]) peers[p.to]->on_packet(p.from, p.data.data(), p.data.size());
            } else {
                ++i;
            }
        }
        for (std::size_t i = 0; i < peers.size(); ++i)
            if (up[i]) peers[i]->tick(now);
        if (check_masters) count_double_masters();
    }
    void run(std::uint64_t ms) {
        for (std::uint64_t t = 0; t < ms; t += 10) step(10);
    }
    void count_double_masters() {
        std::map<NetId, int> masters;
        for (std::size_t i = 0; i < peers.size(); ++i) {
            if (!up[i]) continue;
            for (auto& kv : games[i]->ents)
                if (kv.second.loaded && peers[i]->role(kv.first) == Role::Master) ++masters[kv.first];
        }
        for (auto& kv : masters)
            if (kv.second > 1) ++double_masters;
    }
    // Place an entity on every listed peer and load it there.
    void load(int peer, NetId id, Vec3 at) {
        FakeEntity& e = games[peer]->ents[id];
        e.pos = at;
        e.loaded = true;
        peers[peer]->entity_loaded(id);
    }
    void unload(int peer, NetId id) {
        peers[peer]->entity_unloaded(id);
        games[peer]->ents[id].loaded = false;
    }
    void move_player(int peer, float x) { peers[peer]->set_player_position(Vec3{x, 0, 0}); }
};

void SimTransport::send(PeerId to, const std::uint8_t* d, std::size_t n) { sim->enqueue(self, to, d, n); }

// ---- tests -------------------------------------------------------------------------------------------------
static void test_wire() {
    Writer w;
    w.u8(7); w.u16(0xBEEF); w.u32(0xDEADBEEF); w.u64(0x0123456789ABCDEFull); w.i32(-5); w.f32(1.5f);
    w.vec3(Vec3{1, 2, 3});
    Reader r(w.data().data(), w.data().size());
    CHECK(r.u8() == 7);
    CHECK(r.u16() == 0xBEEF);
    CHECK(r.u32() == 0xDEADBEEF);
    CHECK(r.u64() == 0x0123456789ABCDEFull);
    CHECK(r.i32() == -5);
    CHECK(r.f32() == 1.5f);
    const Vec3 v = r.vec3();
    CHECK(v.x == 1 && v.y == 2 && v.z == 3);
    CHECK(r.ok() && r.remaining() == 0);
    Reader t(w.data().data(), 5);  // truncated
    t.u8(); t.u16(); t.u32();
    CHECK(!t.ok());
    CHECK(t.u64() == 0);
}

static void test_lane_loss() {
    Lane a, b;
    Rng rng(99);
    std::vector<std::vector<std::uint8_t>> to_b, to_a, got;
    std::deque<std::pair<std::uint64_t, std::vector<std::uint8_t>>> ab, ba;
    std::uint64_t now = 0;
    const int N = 1000;
    for (int i = 0; i < N; ++i) {
        Writer w;
        w.u32(static_cast<std::uint32_t>(i));
        to_b.clear();
        a.push(w.data(), now, to_b);
        for (auto& p : to_b) if (rng.unit() > 0.3f) ab.push_back({now + 20 + rng.next() % 40, p});
    }
    for (int iter = 0; iter < 2000 && got.size() < static_cast<std::size_t>(N); ++iter) {
        now += 10;
        to_b.clear();
        a.tick(now, to_b);
        for (auto& p : to_b) if (rng.unit() > 0.3f) ab.push_back({now + 20 + rng.next() % 40, p});
        to_a.clear();
        b.tick(now, to_a);
        for (auto& p : to_a) if (rng.unit() > 0.3f) ba.push_back({now + 20 + rng.next() % 40, p});
        for (std::size_t i = 0; i < ab.size();) {
            if (ab[i].first <= now) { b.receive(ab[i].second.data(), ab[i].second.size(), got); ab.erase(ab.begin() + static_cast<std::ptrdiff_t>(i)); }
            else ++i;
        }
        std::vector<std::vector<std::uint8_t>> none;
        for (std::size_t i = 0; i < ba.size();) {
            if (ba[i].first <= now) { a.receive(ba[i].second.data(), ba[i].second.size(), none); ba.erase(ba.begin() + static_cast<std::ptrdiff_t>(i)); }
            else ++i;
        }
    }
    CHECK(got.size() == static_cast<std::size_t>(N));
    bool in_order = true;
    for (std::size_t i = 0; i < got.size(); ++i) {
        Reader r(got[i].data(), got[i].size());
        if (r.u32() != i) in_order = false;
    }
    CHECK(in_order);
    CHECK(a.resent() > 0);
}

static void test_garbage_packets() {
    Sim s(2);
    s.connect_all();
    Rng rng(7);
    for (int i = 0; i < 2000; ++i) {
        std::vector<std::uint8_t> junk(rng.next() % 40);
        for (auto& b : junk) b = static_cast<std::uint8_t>(rng.next());
        if (i % 3 == 0 && junk.size() >= 4) { junk[0] = 'A'; junk[1] = 'C'; junk[2] = 1; junk[3] = static_cast<std::uint8_t>(rng.next() % 3); }
        s.peers[0]->on_packet(1, junk.data(), junk.size());
    }
    CHECK(s.peers[0]->stats().bad_packets > 0);
}

// Entity at the origin; players on the x axis, so x is the distance to it.
static const NetId BELL = 0x5106001A;

static void settle_owner(Sim& s, int expect_owner, const char* what, int line) {
    s.run(3000);
    bool ok = s.peers[0]->owner(BELL) == expect_owner && s.peers[1]->owner(BELL) == expect_owner;
    ++g_checks;
    if (!ok) {
        ++g_failed;
        std::printf("  FAIL line %d (%s): owner seen by host %d, joiner %d, expected %d\n", line, what,
                    s.peers[0]->owner(BELL), s.peers[1]->owner(BELL), expect_owner);
    }
}
#define OWNER(sim, who, what) settle_owner(sim, who, what, __LINE__)

static void test_acu_bell_sequences() {
    Sim s(2);
    s.connect_all();
    const int HOST = 0, JOIN = 1;
    // Joiner loads first and owns the bell (in ACU the joiner owned it at the start of migrate1).
    s.move_player(HOST, 88.8f);
    s.move_player(JOIN, 30.0f);
    s.run(200);
    s.load(JOIN, BELL, Vec3{0, 0, 0});
    s.run(200);
    s.load(HOST, BELL, Vec3{0, 0, 0});
    OWNER(s, JOIN, "start");

    // migrate1 outbound: the joiner keeps it out to 100 m, even beyond the host's 88.8 m.
    for (float d : {45.0f, 58.6f, 62.5f, 88.2f, 100.0f}) {
        s.move_player(JOIN, d);
        OWNER(s, JOIN, "outbound, joiner keeps it");
    }
    // 111 m: the joiner's game unloads it and the host takes over.
    s.move_player(JOIN, 111.0f);
    s.unload(JOIN, BELL);
    OWNER(s, HOST, "unload at 111 m hands over");

    // migrate2 inbound with the host fixed at 88.8 m: claim between 49.83 and 52.56 m.
    s.move_player(JOIN, 100.0f);
    s.load(JOIN, BELL, Vec3{0, 0, 0});
    for (float d : {100.0f, 88.2f, 62.5f, 58.56f, 55.47f, 52.56f}) {
        s.move_player(JOIN, d);
        OWNER(s, HOST, "inbound, host keeps it");
    }
    s.move_player(JOIN, 49.83f);
    OWNER(s, JOIN, "inbound 49.83 m claims");

    // migrate3 A: the owner is inside the claim distance; a nearer joiner does not take it.
    s.move_player(JOIN, 111.0f);
    s.unload(JOIN, BELL);
    OWNER(s, HOST, "A: give host ownership");
    s.move_player(HOST, 47.5f);
    s.run(500);
    s.move_player(JOIN, 29.99f);
    s.load(JOIN, BELL, Vec3{0, 0, 0});
    OWNER(s, HOST, "A: owner inside, joiner nearer");

    // migrate3 B: owner just outside the claim distance but inside the release distance.
    s.move_player(JOIN, 111.0f);
    s.unload(JOIN, BELL);
    OWNER(s, HOST, "B: give host ownership");
    s.move_player(HOST, 55.48f);
    s.run(500);
    s.move_player(JOIN, 49.83f);
    s.load(JOIN, BELL, Vec3{0, 0, 0});
    OWNER(s, HOST, "B: owner at 55.48 m keeps it");
    s.move_player(HOST, 88.83f);
    OWNER(s, JOIN, "B: owner leaves to 88.83 m");

    // Danton (observation 18): owner at 59 m, other player at 42 m -> moves (needs release <= 59).
    s.move_player(JOIN, 111.0f);
    s.unload(JOIN, BELL);
    OWNER(s, HOST, "Danton: host owns");
    s.move_player(HOST, 59.0f);
    s.move_player(JOIN, 42.0f);
    s.load(JOIN, BELL, Vec3{0, 0, 0});
    OWNER(s, JOIN, "Danton: 42 m vs 59 m");

    CHECK(s.double_masters == 0);
    // Roles reported to the games match the session.
    CHECK(s.games[JOIN]->ents[BELL].role == Role::Master);
    CHECK(s.games[HOST]->ents[BELL].role == Role::Replica);
}

static void test_announce_races() {
    // Both load before they are connected; after connecting, peer 0 must be the only master.
    {
        Sim s(2);
        s.check_masters = false;
        s.load(0, 1, Vec3{});
        s.load(1, 1, Vec3{});
        s.connect_all();
        s.run(1000);
        CHECK(s.peers[0]->role(1) == Role::Master);
        CHECK(s.peers[1]->role(1) == Role::Replica);
        CHECK(s.games[1]->ents[1].role == Role::Replica);
    }
    // Both load at the same instant while connected.
    {
        Sim s(2);
        s.check_masters = false;
        s.connect_all();
        s.run(200);
        s.load(1, 2, Vec3{});
        s.load(0, 2, Vec3{});
        s.run(1000);
        CHECK(s.peers[0]->role(2) == Role::Master);
        CHECK(s.peers[1]->role(2) == Role::Replica);
    }
}

static void test_damage_authority() {
    Sim s(2);
    s.connect_all();
    s.move_player(0, 5);
    s.move_player(1, 5);
    s.run(200);
    s.load(0, 9, Vec3{});
    s.run(200);
    s.load(1, 9, Vec3{});
    s.run(500);
    CHECK(s.peers[0]->role(9) == Role::Master);
    // The replica's hit becomes a request; only the owner applies it; both show the owner's value.
    s.peers[1]->local_hit(9, 30, 0xA77AC4E5);
    s.run(500);
    CHECK(s.games[0]->ents[9].damage_applied == 1);
    CHECK(s.games[1]->ents[9].damage_applied == 0);
    CHECK(s.games[0]->ents[9].life == 70);
    CHECK(s.games[1]->ents[9].life == 70);
    // Two lethal hits at once from both machines: exactly one death on each machine, decided by the owner.
    s.peers[0]->local_hit(9, 80, 1);
    s.peers[1]->local_hit(9, 80, 2);
    s.run(1000);
    CHECK(s.games[0]->ents[9].deaths == 1);
    CHECK(s.games[1]->ents[9].deaths == 1);
    CHECK(s.games[0]->ents[9].damage_applied == 2);  // the second request reached a dead entity and was dropped
    CHECK(s.peers[0]->state(9).dead && s.peers[1]->state(9).dead);
    CHECK(s.peers[0]->stats().damage_dropped >= 1);
}

static void test_damage_during_handover() {
    Sim s(3);
    s.connect_all();
    // Owner 0 is far, 1 claims; 2 hits while the grant is in flight and its request goes to the old owner.
    s.move_player(0, 90);
    s.move_player(1, 60);
    s.move_player(2, 70);
    s.run(200);
    s.load(0, 5, Vec3{});
    s.run(200);
    s.load(1, 5, Vec3{});
    s.load(2, 5, Vec3{});
    s.run(500);
    CHECK(s.peers[2]->owner(5) == 0);
    s.move_player(1, 10);
    // Let the claim reach owner 0, then hit from 2 before 2 has seen the grant.
    for (int i = 0; i < 600 && s.peers[0]->owner(5) == 0; ++i) s.step(5);
    CHECK(s.peers[0]->owner(5) == 1);
    CHECK(s.peers[2]->owner(5) == 0);
    s.peers[2]->local_hit(5, 25, 3);
    s.run(1000);
    CHECK(s.peers[0]->stats().damage_forwarded == 1);
    int applied = 0;
    for (auto& g : s.games) applied += g->ents[5].damage_applied;
    CHECK(applied == 1);
    CHECK(s.games[1]->ents[5].damage_applied == 1);
    CHECK(s.games[0]->ents[5].life == 75 && s.games[2]->ents[5].life == 75);
    CHECK(s.double_masters == 0);
}

static void test_join_resync() {
    Sim s(3);
    // Peers 0 and 1 play; peer 2 joins later.
    s.peers[0]->connect_peer(1);
    s.peers[1]->connect_peer(0);
    s.move_player(0, 5);
    s.move_player(1, 80);
    s.run(200);
    s.load(0, 11, Vec3{});
    s.run(200);
    s.peers[0]->local_hit(11, 40, 4);
    s.run(500);
    s.peers[2]->connect_peer(0);
    s.peers[2]->connect_peer(1);
    s.peers[0]->connect_peer(2);
    s.peers[1]->connect_peer(2);
    s.move_player(2, 70);
    s.run(500);
    s.load(2, 11, Vec3{});
    s.run(1000);
    CHECK(s.peers[2]->owner(11) == 0);
    CHECK(s.peers[2]->role(11) == Role::Replica);
    CHECK(s.games[2]->ents[11].life == 60);
    CHECK(s.double_masters == 0);
}

static void test_owner_disconnect() {
    Sim s(3);
    s.check_masters = false;
    s.connect_all();
    for (int i = 0; i < 3; ++i) s.move_player(i, 20.0f + static_cast<float>(i));
    s.run(200);
    s.load(0, 21, Vec3{});
    s.run(200);
    s.load(1, 21, Vec3{});
    s.load(2, 21, Vec3{});
    s.run(500);
    CHECK(s.peers[1]->owner(21) == 0);
    s.up[0] = false;
    s.peers[1]->disconnect_peer(0);
    s.peers[2]->disconnect_peer(0);
    s.run(2000);
    // Both survivors took over at the same epoch; the lower id must win on both.
    CHECK(s.peers[1]->owner(21) == 1);
    CHECK(s.peers[2]->owner(21) == 1);
    CHECK(s.peers[1]->role(21) == Role::Master);
    CHECK(s.peers[2]->role(21) == Role::Replica);
}

static void test_random_walk() {
    SessionConfig cfg;
    Sim s(2, cfg);
    s.loss = 0.2f;
    s.jitter_ms = 60;
    s.connect_all();
    Rng rng(2026);
    const int E = 24;
    for (int e = 1; e <= E; ++e) {
        const Vec3 at{(rng.unit() - 0.5f) * 300, (rng.unit() - 0.5f) * 300, 0};
        s.load(0, static_cast<NetId>(e), at);
        s.load(1, static_cast<NetId>(e), at);
    }
    s.check_masters = false;
    s.run(2000);  // announce races settle
    s.check_masters = true;
    float px[2] = {0, 0}, py[2] = {0, 0};
    for (int t = 0; t < 600; ++t) {
        for (int p = 0; p < 2; ++p) {
            px[p] += (rng.unit() - 0.5f) * 8;
            py[p] += (rng.unit() - 0.5f) * 8;
            s.peers[p]->set_player_position(Vec3{px[p], py[p], 0});
        }
        if (rng.unit() < 0.2f) {
            const NetId id = 1 + rng.next() % E;
            s.peers[rng.next() % 2]->local_hit(id, 1 + static_cast<std::int32_t>(rng.next() % 15), 77);
        }
        s.run(100);
    }
    s.loss = 0;
    s.run(5000);
    int agree = 0, one_master = 0, life_agree = 0, deaths_ok = 0;
    for (int e = 1; e <= E; ++e) {
        const NetId id = static_cast<NetId>(e);
        agree += s.peers[0]->owner(id) == s.peers[1]->owner(id);
        one_master += (s.peers[0]->role(id) == Role::Master) + (s.peers[1]->role(id) == Role::Master) == 1;
        life_agree += s.games[0]->ents[id].life == s.games[1]->ents[id].life;
        const int d0 = s.games[0]->ents[id].deaths, d1 = s.games[1]->ents[id].deaths;
        deaths_ok += d0 == d1 && d0 <= 1 && (d0 == 1) == (s.games[0]->ents[id].life <= 0);
    }
    CHECK(agree == E);
    CHECK(one_master == E);
    CHECK(life_agree == E);
    CHECK(deaths_ok == E);
    CHECK(s.double_masters == 0);
    std::printf("  random walk: grants %llu+%llu, claims refused %llu+%llu, forwarded %llu+%llu\n",
                (unsigned long long)s.peers[0]->stats().grants_sent, (unsigned long long)s.peers[1]->stats().grants_sent,
                (unsigned long long)s.peers[0]->stats().claims_refused, (unsigned long long)s.peers[1]->stats().claims_refused,
                (unsigned long long)s.peers[0]->stats().damage_forwarded, (unsigned long long)s.peers[1]->stats().damage_forwarded);
}

int main() {
    struct T { const char* name; void (*fn)(); } tests[] = {
        {"wire", test_wire},
        {"lane under 30% loss", test_lane_loss},
        {"garbage packets", test_garbage_packets},
        {"ACU bell sequences (ACU-RE-0092 obs 19-20)", test_acu_bell_sequences},
        {"announce races", test_announce_races},
        {"damage authority", test_damage_authority},
        {"damage during hand-over", test_damage_during_handover},
        {"join resync", test_join_resync},
        {"owner disconnect", test_owner_disconnect},
        {"random walk, 20% loss", test_random_walk},
    };
    for (auto& t : tests) {
        const int before = g_failed;
        std::printf("%s\n", t.name);
        t.fn();
        std::printf("  %s\n", g_failed == before ? "ok" : "FAILED");
    }
    std::printf("%d checks, %d failed\n", g_checks, g_failed);
    return g_failed ? 1 : 0;
}
