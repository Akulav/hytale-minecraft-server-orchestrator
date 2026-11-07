//! Telemetry sampler: every 2 s the machine's CPU / RAM and each running server's CPU / RAM / players.

use std::time::Duration;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

use crate::state::{HISTORY_CAP, Sample, Shared, now_ms};

pub fn spawn(state: Shared) {
    std::thread::Builder::new()
        .name("telemetry".into())
        .spawn(move || {
            let mut sys = System::new_with_specifics(RefreshKind::nothing());
            let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).max(1);
            loop {
                if state.is_quitting() {
                    break;
                }
                sample(&state, &mut sys, cpus);
                std::thread::sleep(Duration::from_secs(2));
            }
        })
        .expect("telemetry thread");
}

fn sample(state: &Shared, sys: &mut System, cpus: usize) {
    sys.refresh_cpu_usage();
    sys.refresh_memory();
    let pids: Vec<(String, u32)> = state.runtimes.read().iter().filter_map(|(id, r)| r.pid.map(|p| (id.clone(), p))).collect();
    let players: usize = state.runtimes.read().values().map(|r| r.players.len()).sum();
    let list: Vec<Pid> = pids.iter().map(|(_, p)| Pid::from_u32(*p)).collect();
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&list), true, ProcessRefreshKind::nothing().with_cpu().with_memory());
    {
        let mut h = state.host.write();
        h.cpus = cpus;
        h.cpu = sys.global_cpu_usage();
        h.ram_total_mb = sys.total_memory() / (1024 * 1024);
        h.ram_used_mb = sys.used_memory() / (1024 * 1024);
        let ram_pct = if h.ram_total_mb > 0 { h.ram_used_mb as f32 * 100.0 / h.ram_total_mb as f32 } else { 0.0 };
        let cpu = h.cpu;
        h.history.push_back((cpu, ram_pct, players as f32));
        while h.history.len() > HISTORY_CAP {
            h.history.pop_front();
        }
    }
    let now = now_ms();
    for (id, pid) in pids {
        let (cpu, ram) = sys.process(Pid::from_u32(pid)).map(|p| (p.cpu_usage() / cpus as f32, p.memory() / (1024 * 1024))).unwrap_or((0.0, 0));
        state.with_runtime(&id, |r| {
            if r.pid != Some(pid) {
                return;
            }
            r.cpu = cpu;
            r.ram_mb = ram;
            let players = r.players.len();
            r.history.push_back(Sample { at: now, cpu, ram_mb: ram, players });
            while r.history.len() > HISTORY_CAP {
                r.history.pop_front();
            }
        });
    }
    state.notify();
}
