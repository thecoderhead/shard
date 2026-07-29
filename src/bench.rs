use std::time::Instant;

use anyhow::Result;
use crossterm::style::Stylize;

use crate::compact::{self, Archetype};
use crate::tokens;
use crate::ui;

struct BenchSample {
    name: &'static str,
    expected_archetype: Archetype,
    input: &'static str,
}

const CORPUS: &[BenchSample] = &[
    BenchSample {
        name: "tabular-docker-ps",
        expected_archetype: Archetype::Tabular,
        input: concat!(
            "CONTAINER ID   IMAGE     COMMAND                  CREATED       STATUS       PORTS     NAMES\n",
            "abc123         nginx     \"nginx -g 'daemon of…\"   2 hours ago   Up 2 hours   80/tcp    web-1\n",
            "def456         redis     \"redis-server\"           3 hours ago   Up 3 hours   6379/tcp  cache-1\n",
            "ghi789         postgres  \"postgres -c config…\"    4 hours ago   Up 4 hours   5432/tcp  db-1\n",
            "jkl012         mongo     \"mongod --auth\"          5 hours ago   Up 5 hours   27017/tcp data-1\n",
            "mno345         mysql     \"docker-entrypoint.s…\"   6 hours ago   Up 6 hours   3306/tcp  mysql-1\n",
            "pqr678         rabbitmq  \"docker-entrypoint.s…\"   7 hours ago   Up 7 hours   5672/tcp  queue-1\n",
            "stu901         traefik   \"/entrypoint.sh\"         8 hours ago   Up 8 hours   80/tcp    proxy-1\n",
            "vwx234         prometh   \"/bin/prometheus\"        9 hours ago   Up 9 hours   9090/tcp  monitor-1\n",
            "yza567         grafana   \"/run.sh\"                10 hours ago  Up 10 hours  3000/tcp  dash-1\n",
        ),
    },
    BenchSample {
        name: "linear-cargo-test",
        expected_archetype: Archetype::LinearLog,
        input: concat!(
            "   Compiling shard v0.1.0 (C:\\shard)\n",
            "   Compiling shard v0.1.0 (C:\\shard)\n",
            "   Compiling shard v0.1.0 (C:\\shard)\n",
            "   Compiling shard v0.1.0 (C:\\shard)\n",
            "   Compiling shard v0.1.0 (C:\\shard)\n",
            "   Compiling shard v0.1.0 (C:\\shard)\n",
            "    Finished `test` profile [unoptimized + debuginfo]\n",
            "     Running unittests src/main.rs (target/debug/shard.exe)\n",
            "     Running unittests src/main.rs (target/debug/shard.exe)\n",
            "     Running unittests src/main.rs (target/debug/shard.exe)\n",
        ),
    },
    BenchSample {
        name: "tree-directory",
        expected_archetype: Archetype::Tree,
        input: concat!(
            "src\n",
            "├── main.rs\n",
            "├── cli.rs\n",
            "├── pty\n",
            "│   ├── bridge.rs\n",
            "│   └── mod.rs\n",
            "├── compact\n",
            "│   ├── engine.rs\n",
            "│   ├── classify.rs\n",
            "│   ├── linear.rs\n",
            "│   ├── tabular.rs\n",
            "│   └── tree.rs\n",
            "└── vfs\n",
            "    ├── mod.rs\n",
            "    └── cache.rs\n",
        ),
    },
    BenchSample {
        name: "passthrough-short",
        expected_archetype: Archetype::Passthrough,
        input: "hello world\n",
    },
];

pub fn run() -> Result<()> {
    println!("{}", ui::logo());
    println!("{}", ui::hr("benchmark"));
    println!();

    let mut total_input = 0usize;
    let mut total_output = 0usize;
    let mut total_time_ns = 0u128;
    let mut correct = 0usize;

    for sample in CORPUS {
        let start = Instant::now();
        let result = compact::compact(sample.input, None);
        let elapsed_ns = start.elapsed().as_nanos();

        let input_tokens = tokens::approx(sample.input.as_bytes());
        let output_tokens = tokens::approx(result.text.as_bytes());
        let savings = tokens::savings_pct(input_tokens, output_tokens);

        total_input += sample.input.len();
        total_output += result.text.len();
        total_time_ns += elapsed_ns;

        let arch_ok = result.archetype == sample.expected_archetype;
        if arch_ok {
            correct += 1;
        }

        let status = if arch_ok {
            "●".green().bold().to_string()
        } else {
            format!(
                "● (expected {}, got {})",
                sample.expected_archetype.as_str(),
                result.archetype.as_str()
            )
            .red()
            .bold()
        };

        let bar = ui::progress_bar(savings / 100.0, 12);
        let timing = if elapsed_ns < 1000 {
            format!("{}ns", elapsed_ns)
        } else if elapsed_ns < 1_000_000 {
            format!("{}µs", elapsed_ns / 1000)
        } else {
            format!("{:.1}ms", elapsed_ns as f64 / 1_000_000.0)
        };
        println!(
            "  {}  {:<28}  {bar}  {pct:>5.1}%  {tokens:>12}  {timing:>8}",
            status,
            sample.name.cyan().bold(),
            pct = savings,
            tokens = format!("{}t → {}t", input_tokens, output_tokens),
            timing = timing.dim(),
        );
    }

    println!();
    println!("{}", ui::divider());

    let total_ratio = if total_output > 0 {
        total_input as f64 / total_output as f64
    } else {
        0.0
    };

    let avg_ns = total_time_ns / CORPUS.len() as u128;
    let avg_timing = if avg_ns < 1000 {
        format!("{avg_ns}ns")
    } else if avg_ns < 1_000_000 {
        format!("{}µs", avg_ns / 1000)
    } else {
        format!("{:.1}ms", avg_ns as f64 / 1_000_000.0)
    };

    println!("{}", ui::box_top());
    println!("{}", ui::data_row("Classification accuracy", &format!("{}/{} ({:.0}%)", correct, CORPUS.len(), (correct as f64 / CORPUS.len() as f64) * 100.0).green().bold().to_string()));
    println!("{}", ui::data_row("Avg compression ratio", &format!("{:.1}:1", total_ratio).bold().to_string()));
    println!("{}", ui::data_row("Avg compaction time", &avg_timing.bold().to_string()));
    println!("{}", ui::hex_gauge("Compressed bytes", total_output as u64, total_input.max(1) as u64, "bytes"));
    println!("{}", ui::data_row("Signal", &ui::signal_meter(total_ratio / 10.0_f64.min(1.0))));
    println!("{}", ui::box_bottom());
    println!();

    Ok(())
}
