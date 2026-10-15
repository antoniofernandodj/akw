// Headless runner: shot <rom> <frames> <out.ppm> [button@frame ...]
use sms_wasm::Sms;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut sms = Sms::new(std::fs::read(&a[1]).unwrap());
    let frames: usize = a[2].parse().unwrap();
    let presses: Vec<(u8, usize)> = a[4..]
        .iter()
        .map(|s| {
            let (b, f) = s.split_once('@').unwrap();
            (b.parse().unwrap(), f.parse().unwrap())
        })
        .collect();
    let mut peak = 0f32;
    for f in 0..frames {
        for &(b, at) in &presses {
            if f == at {
                sms.hw.pad[0] &= !(1 << b);
            }
            if f == at + 5 {
                sms.hw.pad[0] |= 1 << b;
            }
        }
        sms.run_frame();
        for s in sms.hw.psg.samples.drain(..) {
            peak = peak.max(s.abs());
        }
    }
    eprintln!("pc={:04x} audio peak={peak:.3}", sms.cpu.pc);
    let mut out = b"P6\n256 192\n255\n".to_vec();
    for px in sms.hw.vdp.fb.chunks(4) {
        out.extend_from_slice(&px[..3]);
    }
    std::fs::write(&a[3], out).unwrap();
}
