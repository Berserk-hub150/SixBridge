use sixbridge::{gpu::Reducer, reference, scene, split_control, Fragment};
use std::{
    error::Error,
    path::{Path, PathBuf},
};

fn verify(label: &str, expected: &[u64], actual: &[u64]) -> Result<(), String> {
    if expected.len() != actual.len() {
        return Err(format!("{label}: readback length mismatch"));
    }
    let mismatches = expected.iter().zip(actual).filter(|(a, b)| a != b).count();
    if let Some(index) = expected.iter().zip(actual).position(|(a, b)| a != b) {
        return Err(format!(
            "{label}: {mismatches} mismatches; texel {index}: expected {:016x}, got {:016x}",
            expected[index], actual[index]
        ));
    }
    println!("{label}: {} texels, 0 mismatches", actual.len());
    Ok(())
}

fn check_cases(gpu: &Reducer) -> Result<(), String> {
    let mut state = 0x6b83_1274u32;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state
    };
    let mut batch = Vec::new();
    for _ in 0..4097 {
        let texel = next() % 31;
        let high = [0, 1, 0x8000_0000, u32::MAX][(next() % 4) as usize];
        batch.push(Fragment::new(
            texel,
            u64::from(high) << 32 | u64::from(next()),
        ));
    }
    // Isolated sentinels exercise a losing low word, zero high word and a full-width tie.
    for (texel, key) in [
        (31, 0x1_ffffffff),
        (31, 0x2_00000000),
        (32, 7),
        (32, 42),
        (33, u64::MAX - 1),
        (33, u64::MAX),
    ] {
        batch.push(Fragment::new(texel, key));
    }
    let expected = reference(35, &batch)?;
    verify("contention", &expected, &gpu.reduce(35, &batch)?)?;
    batch.reverse();
    verify("reversed submissions", &expected, &gpu.reduce(35, &batch)?)?;
    verify("empty batch", &[0, 0], &gpu.reduce(2, &[])?)?;
    Ok(())
}

fn run(out: &Path, software: bool) -> Result<(), Box<dyn Error>> {
    let gpu = pollster::block_on(Reducer::open(software))?;
    println!(
        "Adapter: {} ({:?}, {:?})",
        gpu.adapter.name, gpu.adapter.backend, gpu.adapter.device_type
    );
    check_cases(&gpu)?;
    let fragments = scene::canal();
    let count = scene::WIDTH * scene::HEIGHT;
    let expected = reference(count, &fragments)?;
    let broken = split_control(count, &fragments)?;
    let actual = gpu.reduce(count, &fragments)?;
    let affected = expected.iter().zip(&broken).filter(|(a, b)| a != b).count();
    if affected == 0 {
        return Err("scene no longer exercises the control failure".into());
    }
    std::fs::create_dir_all(out)?;
    scene::write_image(&out.join("split-control.png"), &broken)?;
    scene::write_image(&out.join("gpu.png"), &actual)?;
    scene::write_image(&out.join("reference.png"), &expected)?;
    println!(
        "Canal: {} fragments; independent word maxima corrupt {} texels.",
        fragments.len(),
        affected
    );
    verify("canal GPU readback", &expected, &actual)?;
    println!("Frames written to {}", out.display());
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut software = false;
    let mut output = PathBuf::from("output");
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--software" {
            software = true;
        } else if arg == "--out" {
            output = args.next().ok_or("--out requires a directory")?.into();
        } else if arg == "--help" || arg == "-h" {
            println!("sixbridge [--software] [--out DIRECTORY]");
            return Ok(());
        } else {
            return Err(format!("unknown argument {arg:?}").into());
        }
    }
    run(&output, software)
}
