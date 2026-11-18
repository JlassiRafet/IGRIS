mod common;
use common::{FakeHost, fixture};
use igris_detect::{Gpu, detect};

fn recorded_linux() -> FakeHost {
    let mut h = FakeHost::linux()
        .file("/proc/meminfo", &fixture("linux/meminfo.txt"))
        .cmd(
            "nvidia-smi --query-gpu=name,memory.total --format=csv,noheader,nounits",
            &fixture("linux/nvidia-smi.csv"),
        )
        .cmd("df -Pk /home/user", &fixture("linux/df.txt"));
    h.cores = Some(12);
    h
}

#[test]
fn linux_machine_matches_the_recorded_commands() {
    let m = detect(&recorded_linux()).machine;
    assert_eq!(m.os, "linux");
    assert_eq!(m.cpu_cores, Some(12));
    assert_eq!(m.ram_gb, Some(7.2), "7500660 kB is 7.2 GiB");
    assert_eq!(
        m.gpus,
        Some(vec![Gpu {
            name: "NVIDIA GeForce RTX 3050 Laptop GPU".into(),
            vram_mb: 4096
        }])
    );
    assert_eq!(m.disk_free_gb, Some(384.5), "403175992 KiB free");
}

#[test]
fn a_machine_with_no_answers_is_unknown_not_guessed() {
    let m = detect(&FakeHost::linux()).machine;
    assert_eq!(
        (m.cpu_cores, m.ram_gb, m.gpus, m.disk_free_gb),
        (None, None, None, None)
    );
}

#[test]
fn garbage_output_is_unknown_not_a_crash() {
    let h = FakeHost::linux()
        .file("/proc/meminfo", "nonsense")
        .cmd(
            "nvidia-smi --query-gpu=name,memory.total --format=csv,noheader,nounits",
            "NVIDIA-SMI has failed because it couldn't communicate with the NVIDIA driver.",
        )
        .cmd("df -Pk /home/user", "Filesystem\n");
    let m = detect(&h).machine;
    assert_eq!((m.ram_gb, m.gpus, m.disk_free_gb), (None, None, None));
}

#[test]
fn several_gpus_are_all_listed() {
    let h = FakeHost::linux().cmd(
        "nvidia-smi --query-gpu=name,memory.total --format=csv,noheader,nounits",
        "NVIDIA A, 24576\nNVIDIA B, 8192\n",
    );
    let g = detect(&h).machine.gpus.unwrap();
    assert_eq!(
        g.iter().map(|g| g.vram_mb).collect::<Vec<_>>(),
        [24576, 8192]
    );
}

#[test]
fn macos_reads_memory_from_sysctl() {
    let mut h = FakeHost::linux().cmd("sysctl -n hw.memsize", "17179869184\n");
    h.os = "macos".into();
    assert_eq!(detect(&h).machine.ram_gb, Some(16.0));
}
