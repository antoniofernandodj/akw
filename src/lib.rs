mod psg;
mod sms;
mod vdp;
mod z80;

use wasm_bindgen::prelude::*;

pub use sms::Sms;
pub use vdp::{HEIGHT, WIDTH};

/// Browser-facing wrapper around the emulator.
#[wasm_bindgen]
pub struct Emulator {
    sms: Sms,
}

#[wasm_bindgen]
impl Emulator {
    #[wasm_bindgen(constructor)]
    pub fn new(rom: &[u8]) -> Emulator {
        Emulator { sms: Sms::new(rom.to_vec()) }
    }

    pub fn frame(&mut self) {
        self.sms.run_frame();
    }

    /// RGBA pixels, 256x192.
    pub fn framebuffer(&self) -> Vec<u8> {
        self.sms.hw.vdp.fb.clone()
    }

    /// Mono f32 samples at 44100 Hz produced since the last call.
    pub fn take_audio(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.sms.hw.psg.samples)
    }

    /// 0 up, 1 down, 2 left, 3 right, 4 button 1, 5 button 2.
    pub fn set_button(&mut self, button: u8, down: bool) {
        if button < 8 {
            if down {
                self.sms.hw.pad[0] &= !(1 << button);
            } else {
                self.sms.hw.pad[0] |= 1 << button;
            }
        }
    }

    pub fn pause(&mut self) {
        self.sms.pause();
    }

    pub fn reset(&mut self) {
        self.sms.reset();
    }
}
