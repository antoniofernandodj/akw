//! Master System: memory map, mapper, I/O ports and the frame loop.

use crate::psg::Psg;
use crate::vdp::{Vdp, LINES};
use crate::z80::{Bus, Z80};

const CYCLES_PER_LINE: u64 = 228;

pub struct Hw {
    rom: Vec<u8>,
    ram: [u8; 0x2000],
    cart_ram: Vec<u8>,
    banks: [u8; 3],
    ramctl: u8,
    pub vdp: Vdp,
    pub psg: Psg,
    pub pad: [u8; 2], // active-low
}

impl Hw {
    fn rom_byte(&self, bank: u8, off: usize) -> u8 {
        let nb = ((self.rom.len() + 0x3FFF) / 0x4000).max(1);
        let b = bank as usize % nb;
        self.rom.get(b * 0x4000 + off).copied().unwrap_or(0xFF)
    }
}

impl Bus for Hw {
    fn read(&mut self, a: u16) -> u8 {
        match a {
            0x0000..=0x03FF => self.rom_byte(0, a as usize),
            0x0400..=0x3FFF => self.rom_byte(self.banks[0], a as usize),
            0x4000..=0x7FFF => self.rom_byte(self.banks[1], a as usize - 0x4000),
            0x8000..=0xBFFF => {
                if self.ramctl & 8 != 0 {
                    let page = (self.ramctl as usize >> 2) & 1;
                    self.cart_ram[page * 0x4000 + a as usize - 0x8000]
                } else {
                    self.rom_byte(self.banks[2], a as usize - 0x8000)
                }
            }
            _ => self.ram[a as usize & 0x1FFF],
        }
    }

    fn write(&mut self, a: u16, v: u8) {
        match a {
            0x8000..=0xBFFF => {
                if self.ramctl & 8 != 0 {
                    let page = (self.ramctl as usize >> 2) & 1;
                    self.cart_ram[page * 0x4000 + a as usize - 0x8000] = v;
                }
            }
            0xC000..=0xFFFF => {
                self.ram[a as usize & 0x1FFF] = v;
                match a {
                    0xFFFC => self.ramctl = v,
                    0xFFFD => self.banks[0] = v,
                    0xFFFE => self.banks[1] = v,
                    0xFFFF => self.banks[2] = v,
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn port_in(&mut self, p: u8) -> u8 {
        match p & 0xC1 {
            0x40 => self.vdp.vcounter(),
            0x41 => 0,
            0x80 => self.vdp.read_data(),
            0x81 => self.vdp.read_status(),
            0xC0 => self.pad[0],
            0xC1 => self.pad[1],
            _ => 0xFF,
        }
    }

    fn port_out(&mut self, p: u8, v: u8) {
        match p & 0xC1 {
            0x40 | 0x41 => self.psg.write(v),
            0x80 => self.vdp.write_data(v),
            0x81 => self.vdp.write_ctrl(v),
            _ => {}
        }
    }
}

pub struct Sms {
    pub cpu: Z80,
    pub hw: Hw,
    line_target: u64,
}

impl Sms {
    pub fn new(mut rom: Vec<u8>) -> Self {
        if rom.len() % 0x4000 == 512 {
            rom.drain(..512);
        }
        Sms {
            cpu: Z80::new(),
            hw: Hw {
                rom,
                ram: [0; 0x2000],
                cart_ram: vec![0; 0x8000],
                banks: [0, 1, 2],
                ramctl: 0,
                vdp: Vdp::new(),
                psg: Psg::new(),
                pad: [0xFF, 0xFF],
            },
            line_target: 0,
        }
    }

    pub fn reset(&mut self) {
        let rom = std::mem::take(&mut self.hw.rom);
        *self = Sms::new(rom);
    }

    pub fn pause(&mut self) {
        self.cpu.nmi(&mut self.hw);
    }

    /// Run one full video frame (262 scanlines).
    pub fn run_frame(&mut self) {
        for line in 0..LINES {
            self.hw.vdp.start_line(line);
            self.line_target += CYCLES_PER_LINE;
            let start = self.cpu.cycles;
            while self.cpu.cycles < self.line_target {
                if self.hw.vdp.irq() {
                    self.cpu.irq(&mut self.hw);
                }
                self.cpu.step(&mut self.hw);
            }
            let spent = (self.cpu.cycles - start) as u32;
            self.hw.psg.run(spent);
        }
    }
}
