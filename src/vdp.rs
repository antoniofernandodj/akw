//! Sega Master System VDP (TMS9918-derived, "mode 4"), 256x192.

pub const WIDTH: usize = 256;
pub const HEIGHT: usize = 192;
pub const LINES: usize = 262;

pub struct Vdp {
    pub vram: Vec<u8>,
    pub cram: [u8; 32],
    pub regs: [u8; 16],
    pub fb: Vec<u8>, // RGBA
    addr: u16,
    code: u8,
    latch: bool,
    ctrl_lo: u8,
    buf: u8,
    status: u8,
    line_pending: bool,
    line_counter: u8,
    pub line: usize,
}

impl Vdp {
    pub fn new() -> Self {
        Vdp {
            vram: vec![0; 0x4000],
            cram: [0; 32],
            regs: [0, 0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0xFF, 0, 0, 0, 0, 0],
            fb: vec![0; WIDTH * HEIGHT * 4],
            addr: 0,
            code: 0,
            latch: false,
            ctrl_lo: 0,
            buf: 0,
            status: 0,
            line_pending: false,
            line_counter: 0,
            line: 0,
        }
    }

    pub fn irq(&self) -> bool {
        (self.status & 0x80 != 0 && self.regs[1] & 0x20 != 0)
            || (self.line_pending && self.regs[0] & 0x10 != 0)
    }

    pub fn vcounter(&self) -> u8 {
        let l = self.line;
        if l <= 0xDA {
            l as u8
        } else {
            (l - 6) as u8
        }
    }

    pub fn read_data(&mut self) -> u8 {
        self.latch = false;
        let r = self.buf;
        self.buf = self.vram[self.addr as usize];
        self.addr = (self.addr + 1) & 0x3FFF;
        r
    }

    pub fn write_data(&mut self, v: u8) {
        self.latch = false;
        if self.code == 3 {
            self.cram[(self.addr & 0x1F) as usize] = v;
        } else {
            self.vram[self.addr as usize] = v;
        }
        self.buf = v;
        self.addr = (self.addr + 1) & 0x3FFF;
    }

    pub fn read_status(&mut self) -> u8 {
        self.latch = false;
        let s = self.status;
        self.status = 0;
        self.line_pending = false;
        s | 0x1F
    }

    pub fn write_ctrl(&mut self, v: u8) {
        if !self.latch {
            self.latch = true;
            self.ctrl_lo = v;
            self.addr = (self.addr & 0x3F00) | v as u16;
        } else {
            self.latch = false;
            self.code = v >> 6;
            self.addr = (self.addr & 0x00FF) | ((v as u16 & 0x3F) << 8);
            match self.code {
                0 => {
                    self.buf = self.vram[self.addr as usize];
                    self.addr = (self.addr + 1) & 0x3FFF;
                }
                2 => self.regs[(v & 0x0F) as usize] = self.ctrl_lo,
                _ => {}
            }
        }
    }

    /// Called at the start of every scanline: updates interrupt state and draws visible lines.
    pub fn start_line(&mut self, line: usize) {
        self.line = line;
        if line <= HEIGHT {
            if self.line_counter == 0 {
                self.line_counter = self.regs[10];
                self.line_pending = true;
            } else {
                self.line_counter -= 1;
            }
        } else {
            self.line_counter = self.regs[10];
        }
        if line == HEIGHT {
            self.status |= 0x80;
        }
        if line < HEIGHT {
            self.render_line(line);
        }
    }

    fn rgba(&self, idx: usize) -> [u8; 4] {
        let c = self.cram[idx & 0x1F];
        [(c & 3) * 85, ((c >> 2) & 3) * 85, ((c >> 4) & 3) * 85, 255]
    }

    fn render_line(&mut self, line: usize) {
        let r = self.regs;
        let base = line * WIDTH * 4;
        let backdrop = 16 + (r[7] & 0x0F) as usize;
        let mut out = [backdrop as u8; WIDTH];

        if r[1] & 0x40 != 0 {
            let nt = ((r[2] as usize) & 0x0E) << 10;
            let xscroll = if line < 16 && r[0] & 0x40 != 0 { 0 } else { r[8] };
            let mut bgprio = [false; WIDTH];
            let mut bgcol = [0u8; WIDTH];
            for x in 0..WIDTH {
                if x < 8 && r[0] & 0x20 != 0 {
                    continue; // left column masked with backdrop
                }
                let yscroll = if x >= 192 && r[0] & 0x80 != 0 { 0 } else { r[9] as usize };
                let sx = (x as u8).wrapping_sub(xscroll) as usize;
                let sy = (line + yscroll) % 224;
                let e = nt + (((sy >> 3) * 32 + (sx >> 3)) << 1);
                let (lo, hi) = (self.vram[e], self.vram[e + 1]);
                let tile = lo as usize | ((hi as usize & 1) << 8);
                let mut py = sy & 7;
                if hi & 4 != 0 {
                    py = 7 - py;
                }
                let mut px = sx & 7;
                if hi & 2 == 0 {
                    px = 7 - px;
                }
                let p = tile * 32 + py * 4;
                let v = &self.vram;
                let ci = ((v[p] >> px) & 1)
                    | (((v[p + 1] >> px) & 1) << 1)
                    | (((v[p + 2] >> px) & 1) << 2)
                    | (((v[p + 3] >> px) & 1) << 3);
                out[x] = ci + if hi & 8 != 0 { 16 } else { 0 };
                bgcol[x] = ci;
                bgprio[x] = hi & 0x10 != 0;
            }

            // Sprites
            let big = r[1] & 2 != 0;
            let zoom = if r[1] & 1 != 0 { 2 } else { 1 };
            let h = (if big { 16 } else { 8 }) * zoom;
            let sat = ((r[5] as usize) & 0x7E) << 7;
            let sbase = if r[6] & 4 != 0 { 0x2000 } else { 0 };
            let mut count = 0;
            let mut taken = [false; WIDTH];
            for i in 0..64 {
                let y = self.vram[sat + i];
                if y == 0xD0 {
                    break;
                }
                let top = if y > 0xF0 { y as i32 - 256 } else { y as i32 } + 1;
                let yy = line as i32 - top;
                if yy < 0 || yy >= h as i32 {
                    continue;
                }
                count += 1;
                if count > 8 {
                    self.status |= 0x40;
                    break;
                }
                let sx0 = self.vram[sat + 128 + 2 * i] as i32 - if r[0] & 8 != 0 { 8 } else { 0 };
                let mut tile = self.vram[sat + 129 + 2 * i] as usize;
                if big {
                    tile &= 0xFE;
                }
                let row = yy as usize / zoom;
                let p = sbase + tile * 32 + row * 4;
                for dx in 0..(8 * zoom) as i32 {
                    let xp = sx0 + dx;
                    if !(0..WIDTH as i32).contains(&xp) {
                        continue;
                    }
                    let bit = 7 - (dx as usize / zoom);
                    let v = &self.vram;
                    let ci = ((v[p] >> bit) & 1)
                        | (((v[p + 1] >> bit) & 1) << 1)
                        | (((v[p + 2] >> bit) & 1) << 2)
                        | (((v[p + 3] >> bit) & 1) << 3);
                    if ci == 0 {
                        continue;
                    }
                    let xp = xp as usize;
                    if taken[xp] {
                        self.status |= 0x20;
                        continue;
                    }
                    taken[xp] = true;
                    if bgprio[xp] && bgcol[xp] != 0 {
                        continue;
                    }
                    out[xp] = 16 + ci;
                }
            }
        }

        for x in 0..WIDTH {
            let c = self.rgba(out[x] as usize);
            self.fb[base + x * 4..base + x * 4 + 4].copy_from_slice(&c);
        }
    }
}
