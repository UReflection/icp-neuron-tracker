-- Neuron snapshots table
CREATE TABLE neuron_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    neuron_id TEXT NOT NULL,
    snapshot_date DATE NOT NULL,
    stake_e8s INTEGER NOT NULL,
    maturity_e8s INTEGER NOT NULL,
    staked_maturity_e8s INTEGER NOT NULL,
    voting_power INTEGER NOT NULL,
    age_days INTEGER NOT NULL,
    dissolve_delay_days INTEGER NOT NULL,
    age_bonus_multiplier REAL NOT NULL,
    dissolve_bonus_multiplier REAL NOT NULL,
    state TEXT NOT NULL,
    auto_stake_enabled BOOLEAN NOT NULL,
    created_timestamp INTEGER NOT NULL,
    retrieved_timestamp INTEGER NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(neuron_id, snapshot_date)
);

CREATE INDEX idx_neuron_date ON neuron_snapshots(neuron_id, snapshot_date);
CREATE INDEX idx_snapshot_date ON neuron_snapshots(snapshot_date);

-- Portfolio snapshots table
CREATE TABLE portfolio_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    snapshot_date DATE NOT NULL UNIQUE,
    total_neurons INTEGER NOT NULL,
    total_stake_e8s INTEGER NOT NULL,
    total_maturity_e8s INTEGER NOT NULL,
    total_staked_maturity_e8s INTEGER NOT NULL,
    total_voting_power INTEGER NOT NULL,
    overall_return_percentage REAL NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_portfolio_date ON portfolio_snapshots(snapshot_date);

-- Daily rewards calculation table
CREATE TABLE daily_rewards (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    neuron_id TEXT NOT NULL,
    reward_date DATE NOT NULL,
    maturity_delta_e8s INTEGER NOT NULL,
    staked_maturity_delta_e8s INTEGER NOT NULL,
    total_reward_e8s INTEGER NOT NULL,
    days_elapsed INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(neuron_id, reward_date)
);

CREATE INDEX idx_reward_neuron_date ON daily_rewards(neuron_id, reward_date);