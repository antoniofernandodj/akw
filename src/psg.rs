//! SN76489 programmable sound generator.

const CLOCK: f64 = 3_579_545.0;
pub const SAMPLE_RATE: f64 = 44_100.0;

pub struct Psg {
    tone: [u16; 3],
    att: [u8; 4], // attenuation, 15 = off; index 3 = noise
    noise_ctl: u8,
    latch: u8,
    cnt: [i32; 4],
    out: [bool; 4],
    lfsr: u16,
    tick_acc: f64,
    sample_acc: f64,
    dc_x: f32,
    dc_y: f32,
    vol_table: [f32; 16],
    pub samples: Vec<f32>,
}

impl Psg {
    pub fn new() -> Self {
        let mut vol_table = [0.0; 16];
        for (i, v) in vol_table.iter_mut().enumerate().take(15) {
            *v = 10f32.powf(-0.1 * i as f32);
        }
        Psg {
            tone: [0; 3],
            att: [15; 4],
            noise_ctl: 0,
            latch: 0,
            cnt: [1; 4],
            out: [false; 4],
            lfsr: 0x8000,
            tick_acc: 0.0,
            sample_acc: 0.0,
            dc_x: 0.0,
            dc_y: 0.0,
            vol_table,
            samples: Vec::new(),
        }
    }

    pub fn write(&mut self, v: u8) {
        if v & 0x80 != 0 {
            self.latch = (v >> 4) & 7;
        }
        let ch = (self.latch >> 1) as usize;
        if self.latch & 1 == 1 {
            self.att[ch] = v & 0x0F;
        } else if ch < 3 {
            if v & 0x80 != 0 {
                self.tone[ch] = (self.tone[ch] & 0x3F0) | (v as u16 & 0x0F);
            } else {
                self.tone[ch] = (self.tone[ch] & 0x00F) | ((v as u16 & 0x3F) << 4);
            }
        } else {
            self.noise_ctl = v & 7;
            self.lfsr = 0x8000;
        }
    }

    fn tick(&mut self) {
        for i in 0..3 {
            self.cnt[i] -= 1;
            if self.cnt[i] <= 0 {
                self.cnt[i] = self.tone[i].max(1) as i32;
                self.out[i] = !self.out[i];
            }
        }
        self.cnt[3] -= 1;
        if self.cnt[3] <= 0 {
            self.cnt[3] = match self.noise_ctl & 3 {
                0 => 0x10,
                1 => 0x20,
                2 => 0x40,
                _ => self.tone[2].max(1) as i32,
            };
            self.out[3] = !self.out[3];
            if self.out[3] {
                let fb = if self.noise_ctl & 4 != 0 {
                    ((self.lfsr & 0x0009).count_ones() & 1) as u16
                } else {
                    self.lfsr & 1
                };
                self.lfsr = (self.lfsr >> 1) | (fb << 15);
            }
        }
    }

    fn mix(&self) -> f32 {
        let mut s = 0.0;
        for i in 0..3 {
            let on = self.tone[i] < 2 || self.out[i];
            if on {
                s += self.vol_table[self.att[i] as usize];
            }
        }
        if self.lfsr & 1 != 0 {
            s += self.vol_table[self.att[3] as usize];
        }
        s * 0.25
    }

    /// Advance the chip by `cpu_cycles` Z80 cycles, appending output samples.
    pub fn run(&mut self, cpu_cycles: u32) {
        self.sample_acc += cpu_cycles as f64 * SAMPLE_RATE / CLOCK;
        let n = self.sample_acc as usize;
        self.sample_acc -= n as f64;
        let ticks_per_sample = CLOCK / 16.0 / SAMPLE_RATE;
        for _ in 0..n {
            self.tick_acc += ticks_per_sample;
            let t = self.tick_acc as usize;
            self.tick_acc -= t as f64;
            let mut sum = 0.0;
            for _ in 0..t {
                self.tick();
                sum += self.mix();
            }
            let x = sum / t.max(1) as f32;
            // DC blocker
            let y = x - self.dc_x + 0.995 * self.dc_y;
            self.dc_x = x;
            self.dc_y = y;
            self.samples.push(y);
        }
    }
}
