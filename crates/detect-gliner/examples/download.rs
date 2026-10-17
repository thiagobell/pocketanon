//! Downloads a Registry Model into the default models directory.
//!
//! `cargo run -p pocketanon-detect-gliner --example download -- [model id] [int8|fp16|fp32]`

use std::sync::Mutex;

use pocketanon_detect_gliner::{ModelStore, Quantization, Registry};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let registry = Registry::builtin();
    let spec = match args.next() {
        Some(id) => registry.get(&id).ok_or_else(|| format!("unknown model {id}"))?,
        None => registry.default_model(),
    };
    let q = match args.next().as_deref() {
        None => spec.default_quantization,
        Some("int8") => Quantization::Int8,
        Some("fp16") => Quantization::Fp16,
        Some("fp32") => Quantization::Fp32,
        Some(other) => return Err(format!("unknown variant {other}").into()),
    };
    let store = ModelStore::with_default_dir()?;
    println!("{} ({q}) -> {}", spec.id, store.model_dir(spec).display());
    let last = Mutex::new((String::new(), 0u64));
    let dir = store.download(spec, q, &|p| {
        let mut last = last.lock().unwrap();
        // print every ~25 MB and at file boundaries
        if last.0 != p.file || p.received >= last.1 + 25_000_000 || Some(p.received) == p.total {
            println!("  {} {:>6.1} / {} MB", p.file, p.received as f64 / 1e6, p.total.map_or("?".into(), |t| format!("{:.1}", t as f64 / 1e6)));
            *last = (p.file.clone(), p.received);
        }
    })?;
    println!("status: {:?}", store.status(spec));
    println!("{}", dir.display());
    Ok(())
}
