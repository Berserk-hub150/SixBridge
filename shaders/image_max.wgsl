// Each fragment contributes one unsigned (high, low) key to a linear texel.
// The host zeroes keys before running both passes in submission order.
struct Fragment {
    texel: u32,
    high: u32,
    low: u32,
    padding: u32,
}
struct Key {
    high: atomic<u32>,
    low: atomic<u32>,
}
@group(0) @binding(0) var<storage, read> fragments: array<Fragment>;
@group(0) @binding(1) var<storage, read_write> keys: array<Key>;

@compute @workgroup_size(64)
fn reduce_high(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= arrayLength(&fragments) { return; }
    let f = fragments[id.x];
    atomicMax(&keys[f.texel].high, f.high);
}

@compute @workgroup_size(64)
fn resolve_low(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= arrayLength(&fragments) { return; }
    let f = fragments[id.x];
    if atomicLoad(&keys[f.texel].high) == f.high {
        atomicMax(&keys[f.texel].low, f.low);
    }
}
