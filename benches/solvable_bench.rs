use criterion::{criterion_group, criterion_main, Criterion};
use mindsweeper::server::{local::LocalGame, GameConfig, GameMode, GridConfig, Oracle};

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("new_expert_normal", |b| {
        let game_config = GameConfig {
            grid_config: GridConfig::expert(),
            mode: GameMode::Normal,
            punish_guessing: true,
        };
        b.iter(|| LocalGame::new(game_config, game_config.grid_config.random_tile_id()))
    });
    c.bench_function("new_expert_mindless", |b| {
        let game_config = GameConfig {
            grid_config: GridConfig::expert(),
            mode: GameMode::Mindless,
            punish_guessing: true,
        };
        b.iter(|| LocalGame::new(game_config, game_config.grid_config.random_tile_id()))
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(1_000);
    targets = criterion_benchmark
}
criterion_main!(benches);
