//! Zilog Z80 CPU core (instruction-stepped, cycle-counted).

pub trait Bus {
    fn read(&mut self, addr: u16) -> u8;
    fn write(&mut self, addr: u16, val: u8);
    fn port_in(&mut self, port: u8) -> u8;
    fn port_out(&mut self, port: u8, val: u8);
}

const S: u8 = 0x80;
const Z: u8 = 0x40;
const Y: u8 = 0x20;
const H: u8 = 0x10;
const X: u8 = 0x08;
const PV: u8 = 0x04;
const N: u8 = 0x02;
const C: u8 = 0x01;

fn sz(v: u8) -> u8 {
    (v & (S | Y | X)) | if v == 0 { Z } else { 0 }
}

fn parity(v: u8) -> u8 {
    if v.count_ones() & 1 == 0 {
        PV
    } else {
        0
    }
}

#[derive(Default, Clone)]
pub struct Z80 {
    pub a: u8,
    pub f: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    a2: u8,
    f2: u8,
    b2: u8,
    c2: u8,
    d2: u8,
    e2: u8,
    h2: u8,
    l2: u8,
    pub ix: u16,
    pub iy: u16,
    pub sp: u16,
    pub pc: u16,
    i: u8,
    r: u8,
    iff1: bool,
    iff2: bool,
    im: u8,
    halted: bool,
    ei_delay: bool,
    pub cycles: u64,
}

impl Z80 {
    pub fn new() -> Self {
        let mut z = Z80::default();
        z.reset();
        z
    }

    pub fn reset(&mut self) {
        *self = Z80 {
            a: 0xFF,
            f: 0xFF,
            sp: 0xFFFF,
            ..Z80::default()
        };
    }

    fn bc(&self) -> u16 {
        (self.b as u16) << 8 | self.c as u16
    }
    fn de(&self) -> u16 {
        (self.d as u16) << 8 | self.e as u16
    }
    fn hl(&self) -> u16 {
        (self.h as u16) << 8 | self.l as u16
    }
    fn af(&self) -> u16 {
        (self.a as u16) << 8 | self.f as u16
    }
    fn set_bc(&mut self, v: u16) {
        self.b = (v >> 8) as u8;
        self.c = v as u8;
    }
    fn set_de(&mut self, v: u16) {
        self.d = (v >> 8) as u8;
        self.e = v as u8;
    }
    fn set_hl(&mut self, v: u16) {
        self.h = (v >> 8) as u8;
        self.l = v as u8;
    }
    fn set_af(&mut self, v: u16) {
        self.a = (v >> 8) as u8;
        self.f = v as u8;
    }

    /// HL, IX or IY depending on the active prefix.
    fn idx(&self, pfx: u8) -> u16 {
        match pfx {
            1 => self.ix,
            2 => self.iy,
            _ => self.hl(),
        }
    }
    fn set_idx(&mut self, pfx: u8, v: u16) {
        match pfx {
            1 => self.ix = v,
            2 => self.iy = v,
            _ => self.set_hl(v),
        }
    }

    fn rp(&self, p: u8, pfx: u8) -> u16 {
        match p {
            0 => self.bc(),
            1 => self.de(),
            2 => self.idx(pfx),
            _ => self.sp,
        }
    }
    fn set_rp(&mut self, p: u8, pfx: u8, v: u16) {
        match p {
            0 => self.set_bc(v),
            1 => self.set_de(v),
            2 => self.set_idx(pfx, v),
            _ => self.sp = v,
        }
    }

    /// 8-bit register by index (B C D E H L - A); H/L become IXH/IXL under a prefix.
    fn get8(&self, r: u8, pfx: u8) -> u8 {
        match r {
            0 => self.b,
            1 => self.c,
            2 => self.d,
            3 => self.e,
            4 => {
                if pfx == 0 {
                    self.h
                } else {
                    (self.idx(pfx) >> 8) as u8
                }
            }
            5 => {
                if pfx == 0 {
                    self.l
                } else {
                    self.idx(pfx) as u8
                }
            }
            _ => self.a,
        }
    }
    fn set8(&mut self, r: u8, pfx: u8, v: u8) {
        match r {
            0 => self.b = v,
            1 => self.c = v,
            2 => self.d = v,
            3 => self.e = v,
            4 => {
                if pfx == 0 {
                    self.h = v
                } else {
                    let i = self.idx(pfx);
                    self.set_idx(pfx, (i & 0x00FF) | (v as u16) << 8)
                }
            }
            5 => {
                if pfx == 0 {
                    self.l = v
                } else {
                    let i = self.idx(pfx);
                    self.set_idx(pfx, (i & 0xFF00) | v as u16)
                }
            }
            _ => self.a = v,
        }
    }

    fn cond(&self, c: u8) -> bool {
        match c {
            0 => self.f & Z == 0,
            1 => self.f & Z != 0,
            2 => self.f & C == 0,
            3 => self.f & C != 0,
            4 => self.f & PV == 0,
            5 => self.f & PV != 0,
            6 => self.f & S == 0,
            _ => self.f & S != 0,
        }
    }

    fn inc_r(&mut self) {
        self.r = (self.r & 0x80) | (self.r.wrapping_add(1) & 0x7F);
    }

    fn fetch_op(&mut self, bus: &mut impl Bus) -> u8 {
        let v = bus.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        self.inc_r();
        v
    }
    fn fetch8(&mut self, bus: &mut impl Bus) -> u8 {
        let v = bus.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        v
    }
    fn fetch16(&mut self, bus: &mut impl Bus) -> u16 {
        let lo = self.fetch8(bus) as u16;
        let hi = self.fetch8(bus) as u16;
        hi << 8 | lo
    }
    fn read16(&mut self, bus: &mut impl Bus, a: u16) -> u16 {
        let lo = bus.read(a) as u16;
        let hi = bus.read(a.wrapping_add(1)) as u16;
        hi << 8 | lo
    }
    fn write16(&mut self, bus: &mut impl Bus, a: u16, v: u16) {
        bus.write(a, v as u8);
        bus.write(a.wrapping_add(1), (v >> 8) as u8);
    }
    fn push16(&mut self, bus: &mut impl Bus, v: u16) {
        self.sp = self.sp.wrapping_sub(1);
        bus.write(self.sp, (v >> 8) as u8);
        self.sp = self.sp.wrapping_sub(1);
        bus.write(self.sp, v as u8);
    }
    fn pop16(&mut self, bus: &mut impl Bus) -> u16 {
        let lo = bus.read(self.sp) as u16;
        self.sp = self.sp.wrapping_add(1);
        let hi = bus.read(self.sp) as u16;
        self.sp = self.sp.wrapping_add(1);
        hi << 8 | lo
    }

    /// Address of the (HL) / (IX+d) / (IY+d) operand.
    fn mem_addr(&mut self, bus: &mut impl Bus, pfx: u8, extra: u64) -> u16 {
        if pfx == 0 {
            self.hl()
        } else {
            let d = self.fetch8(bus) as i8;
            self.cycles += extra;
            self.idx(pfx).wrapping_add(d as i16 as u16)
        }
    }

    // ---- ALU ----

    fn alu(&mut self, op: u8, v: u8) {
        let a = self.a;
        match op {
            0 | 1 => {
                let c = if op == 1 { self.f & C } else { 0 };
                let r = a as u16 + v as u16 + c as u16;
                let res = r as u8;
                self.f = sz(res)
                    | if (a & 0xF) + (v & 0xF) + c > 0xF { H } else { 0 }
                    | if (!(a ^ v) & (a ^ res) & 0x80) != 0 { PV } else { 0 }
                    | if r > 0xFF { C } else { 0 };
                self.a = res;
            }
            2 | 3 | 7 => {
                let c = if op == 3 { self.f & C } else { 0 };
                let r = a as i16 - v as i16 - c as i16;
                let res = r as u8;
                let mut f = sz(res)
                    | N
                    | if (a & 0xF) < (v & 0xF) + c { H } else { 0 }
                    | if ((a ^ v) & (a ^ res) & 0x80) != 0 { PV } else { 0 }
                    | if r < 0 { C } else { 0 };
                if op == 7 {
                    f = (f & !(Y | X)) | (v & (Y | X));
                } else {
                    self.a = res;
                }
                self.f = f;
            }
            4 => {
                let res = a & v;
                self.f = sz(res) | H | parity(res);
                self.a = res;
            }
            5 => {
                let res = a ^ v;
                self.f = sz(res) | parity(res);
                self.a = res;
            }
            _ => {
                let res = a | v;
                self.f = sz(res) | parity(res);
                self.a = res;
            }
        }
    }

    fn inc8(&mut self, v: u8) -> u8 {
        let res = v.wrapping_add(1);
        self.f = (self.f & C)
            | sz(res)
            | if v & 0xF == 0xF { H } else { 0 }
            | if v == 0x7F { PV } else { 0 };
        res
    }
    fn dec8(&mut self, v: u8) -> u8 {
        let res = v.wrapping_sub(1);
        self.f = (self.f & C)
            | sz(res)
            | N
            | if v & 0xF == 0 { H } else { 0 }
            | if v == 0x80 { PV } else { 0 };
        res
    }

    fn add16(&mut self, a: u16, b: u16) -> u16 {
        let r = a as u32 + b as u32;
        self.f = (self.f & (S | Z | PV))
            | ((r >> 8) as u8 & (Y | X))
            | if (a & 0xFFF) + (b & 0xFFF) > 0xFFF { H } else { 0 }
            | if r > 0xFFFF { C } else { 0 };
        r as u16
    }

    fn adc16(&mut self, a: u16, b: u16) -> u16 {
        let c = (self.f & C) as u32;
        let r = a as u32 + b as u32 + c;
        let res = r as u16;
        self.f = ((res >> 8) as u8 & (S | Y | X))
            | if res == 0 { Z } else { 0 }
            | if (a & 0xFFF) as u32 + (b & 0xFFF) as u32 + c > 0xFFF { H } else { 0 }
            | if (!(a ^ b) & (a ^ res) & 0x8000) != 0 { PV } else { 0 }
            | if r > 0xFFFF { C } else { 0 };
        res
    }

    fn sbc16(&mut self, a: u16, b: u16) -> u16 {
        let c = (self.f & C) as i32;
        let r = a as i32 - b as i32 - c;
        let res = r as u16;
        self.f = ((res >> 8) as u8 & (S | Y | X))
            | if res == 0 { Z } else { 0 }
            | N
            | if ((a & 0xFFF) as i32) < (b & 0xFFF) as i32 + c { H } else { 0 }
            | if ((a ^ b) & (a ^ res) & 0x8000) != 0 { PV } else { 0 }
            | if r < 0 { C } else { 0 };
        res
    }

    fn daa(&mut self) {
        let a = self.a;
        let mut corr = 0u8;
        let mut carry = self.f & C != 0;
        if self.f & H != 0 || (a & 0xF) > 9 {
            corr |= 0x06;
        }
        if carry || a > 0x99 {
            corr |= 0x60;
            carry = true;
        }
        let sub = self.f & N != 0;
        let res = if sub { a.wrapping_sub(corr) } else { a.wrapping_add(corr) };
        let half = if sub {
            self.f & H != 0 && (a & 0xF) < 6
        } else {
            (a & 0xF) > 9
        };
        self.f = sz(res)
            | parity(res)
            | (self.f & N)
            | if half { H } else { 0 }
            | if carry { C } else { 0 };
        self.a = res;
    }

    fn cb_op(&mut self, x: u8, y: u8, v: u8) -> u8 {
        match x {
            0 => {
                let (res, c) = match y {
                    0 => (v.rotate_left(1), v >> 7),
                    1 => (v.rotate_right(1), v & 1),
                    2 => ((v << 1) | (self.f & C), v >> 7),
                    3 => ((v >> 1) | ((self.f & C) << 7), v & 1),
                    4 => (v << 1, v >> 7),
                    5 => ((v >> 1) | (v & 0x80), v & 1),
                    6 => ((v << 1) | 1, v >> 7),
                    _ => (v >> 1, v & 1),
                };
                self.f = sz(res) | parity(res) | c;
                res
            }
            1 => {
                let bit = v & (1 << y);
                self.f = (self.f & C)
                    | H
                    | (v & (Y | X))
                    | if bit == 0 { Z | PV } else { 0 }
                    | if y == 7 && bit != 0 { S } else { 0 };
                v
            }
            2 => v & !(1 << y),
            _ => v | (1 << y),
        }
    }

    // ---- Execution ----

    /// Execute one instruction (or one idle HALT cycle group). Returns cycles consumed.
    pub fn step(&mut self, bus: &mut impl Bus) -> u32 {
        let start = self.cycles;
        let was_ei = self.ei_delay;
        if self.halted {
            self.cycles += 4;
            self.inc_r();
        } else {
            let mut pfx = 0u8;
            loop {
                let op = self.fetch_op(bus);
                match op {
                    0xDD => {
                        pfx = 1;
                        self.cycles += 4;
                    }
                    0xFD => {
                        pfx = 2;
                        self.cycles += 4;
                    }
                    _ => {
                        self.exec(bus, op, pfx);
                        break;
                    }
                }
            }
        }
        if was_ei {
            self.ei_delay = false;
        }
        (self.cycles - start) as u32
    }

    /// Maskable interrupt request. Returns true if it was accepted.
    pub fn irq(&mut self, bus: &mut impl Bus) -> bool {
        if !self.iff1 || self.ei_delay {
            return false;
        }
        self.halted = false;
        self.iff1 = false;
        self.iff2 = false;
        self.inc_r();
        let pc = self.pc;
        self.push16(bus, pc);
        if self.im == 2 {
            let a = (self.i as u16) << 8 | 0xFF;
            self.pc = self.read16(bus, a);
            self.cycles += 19;
        } else {
            self.pc = 0x38;
            self.cycles += 13;
        }
        true
    }

    pub fn nmi(&mut self, bus: &mut impl Bus) {
        self.halted = false;
        self.iff2 = self.iff1;
        self.iff1 = false;
        self.inc_r();
        let pc = self.pc;
        self.push16(bus, pc);
        self.pc = 0x66;
        self.cycles += 11;
    }

    fn exec(&mut self, bus: &mut impl Bus, op: u8, pfx: u8) {
        let x = op >> 6;
        let y = (op >> 3) & 7;
        let z = op & 7;
        let p = y >> 1;
        let q = y & 1;
        match x {
            0 => match z {
                0 => match y {
                    0 => self.cycles += 4,
                    1 => {
                        std::mem::swap(&mut self.a, &mut self.a2);
                        std::mem::swap(&mut self.f, &mut self.f2);
                        self.cycles += 4;
                    }
                    2 => {
                        let d = self.fetch8(bus) as i8;
                        self.b = self.b.wrapping_sub(1);
                        if self.b != 0 {
                            self.pc = self.pc.wrapping_add(d as i16 as u16);
                            self.cycles += 13;
                        } else {
                            self.cycles += 8;
                        }
                    }
                    3 => {
                        let d = self.fetch8(bus) as i8;
                        self.pc = self.pc.wrapping_add(d as i16 as u16);
                        self.cycles += 12;
                    }
                    _ => {
                        let d = self.fetch8(bus) as i8;
                        if self.cond(y - 4) {
                            self.pc = self.pc.wrapping_add(d as i16 as u16);
                            self.cycles += 12;
                        } else {
                            self.cycles += 7;
                        }
                    }
                },
                1 => {
                    if q == 0 {
                        let nn = self.fetch16(bus);
                        self.set_rp(p, pfx, nn);
                        self.cycles += 10;
                    } else {
                        let a = self.idx(pfx);
                        let b = self.rp(p, pfx);
                        let r = self.add16(a, b);
                        self.set_idx(pfx, r);
                        self.cycles += 11;
                    }
                }
                2 => match (q, p) {
                    (0, 0) => {
                        bus.write(self.bc(), self.a);
                        self.cycles += 7;
                    }
                    (0, 1) => {
                        bus.write(self.de(), self.a);
                        self.cycles += 7;
                    }
                    (0, 2) => {
                        let nn = self.fetch16(bus);
                        let v = self.idx(pfx);
                        self.write16(bus, nn, v);
                        self.cycles += 16;
                    }
                    (0, _) => {
                        let nn = self.fetch16(bus);
                        bus.write(nn, self.a);
                        self.cycles += 13;
                    }
                    (_, 0) => {
                        self.a = bus.read(self.bc());
                        self.cycles += 7;
                    }
                    (_, 1) => {
                        self.a = bus.read(self.de());
                        self.cycles += 7;
                    }
                    (_, 2) => {
                        let nn = self.fetch16(bus);
                        let v = self.read16(bus, nn);
                        self.set_idx(pfx, v);
                        self.cycles += 16;
                    }
                    _ => {
                        let nn = self.fetch16(bus);
                        self.a = bus.read(nn);
                        self.cycles += 13;
                    }
                },
                3 => {
                    let v = self.rp(p, pfx);
                    let v = if q == 0 { v.wrapping_add(1) } else { v.wrapping_sub(1) };
                    self.set_rp(p, pfx, v);
                    self.cycles += 6;
                }
                4 | 5 => {
                    let dec = z == 5;
                    if y == 6 {
                        let addr = self.mem_addr(bus, pfx, 8);
                        let v = bus.read(addr);
                        let r = if dec { self.dec8(v) } else { self.inc8(v) };
                        bus.write(addr, r);
                        self.cycles += 11;
                    } else {
                        let v = self.get8(y, pfx);
                        let r = if dec { self.dec8(v) } else { self.inc8(v) };
                        self.set8(y, pfx, r);
                        self.cycles += 4;
                    }
                }
                6 => {
                    if y == 6 {
                        let addr = self.mem_addr(bus, pfx, 5);
                        let n = self.fetch8(bus);
                        bus.write(addr, n);
                        self.cycles += 10;
                    } else {
                        let n = self.fetch8(bus);
                        self.set8(y, pfx, n);
                        self.cycles += 7;
                    }
                }
                _ => {
                    self.cycles += 4;
                    let a = self.a;
                    match y {
                        0 => {
                            let c = a >> 7;
                            self.a = a.rotate_left(1);
                            self.f = (self.f & (S | Z | PV)) | (self.a & (Y | X)) | c;
                        }
                        1 => {
                            let c = a & 1;
                            self.a = a.rotate_right(1);
                            self.f = (self.f & (S | Z | PV)) | (self.a & (Y | X)) | c;
                        }
                        2 => {
                            let c = a >> 7;
                            self.a = (a << 1) | (self.f & C);
                            self.f = (self.f & (S | Z | PV)) | (self.a & (Y | X)) | c;
                        }
                        3 => {
                            let c = a & 1;
                            self.a = (a >> 1) | ((self.f & C) << 7);
                            self.f = (self.f & (S | Z | PV)) | (self.a & (Y | X)) | c;
                        }
                        4 => self.daa(),
                        5 => {
                            self.a = !a;
                            self.f = (self.f & (S | Z | PV | C)) | H | N | (self.a & (Y | X));
                        }
                        6 => {
                            self.f = (self.f & (S | Z | PV)) | (a & (Y | X)) | C;
                        }
                        _ => {
                            let oc = self.f & C;
                            self.f = (self.f & (S | Z | PV))
                                | (a & (Y | X))
                                | if oc != 0 { H } else { 0 }
                                | (oc ^ C);
                        }
                    }
                }
            },
            1 => {
                if op == 0x76 {
                    self.halted = true;
                    self.cycles += 4;
                } else if y == 6 {
                    let addr = self.mem_addr(bus, pfx, 8);
                    let v = self.get8(z, 0);
                    bus.write(addr, v);
                    self.cycles += 7;
                } else if z == 6 {
                    let addr = self.mem_addr(bus, pfx, 8);
                    let v = bus.read(addr);
                    self.set8(y, 0, v);
                    self.cycles += 7;
                } else {
                    let v = self.get8(z, pfx);
                    self.set8(y, pfx, v);
                    self.cycles += 4;
                }
            }
            2 => {
                let v = if z == 6 {
                    let addr = self.mem_addr(bus, pfx, 8);
                    self.cycles += 7;
                    bus.read(addr)
                } else {
                    self.cycles += 4;
                    self.get8(z, pfx)
                };
                self.alu(y, v);
            }
            _ => match z {
                0 => {
                    if self.cond(y) {
                        self.pc = self.pop16(bus);
                        self.cycles += 11;
                    } else {
                        self.cycles += 5;
                    }
                }
                1 => {
                    if q == 0 {
                        let v = self.pop16(bus);
                        if p == 3 {
                            self.set_af(v);
                        } else {
                            self.set_rp(p, pfx, v);
                        }
                        self.cycles += 10;
                    } else {
                        match p {
                            0 => {
                                self.pc = self.pop16(bus);
                                self.cycles += 10;
                            }
                            1 => {
                                std::mem::swap(&mut self.b, &mut self.b2);
                                std::mem::swap(&mut self.c, &mut self.c2);
                                std::mem::swap(&mut self.d, &mut self.d2);
                                std::mem::swap(&mut self.e, &mut self.e2);
                                std::mem::swap(&mut self.h, &mut self.h2);
                                std::mem::swap(&mut self.l, &mut self.l2);
                                self.cycles += 4;
                            }
                            2 => {
                                self.pc = self.idx(pfx);
                                self.cycles += 4;
                            }
                            _ => {
                                self.sp = self.idx(pfx);
                                self.cycles += 6;
                            }
                        }
                    }
                }
                2 => {
                    let nn = self.fetch16(bus);
                    if self.cond(y) {
                        self.pc = nn;
                    }
                    self.cycles += 10;
                }
                3 => match y {
                    0 => {
                        self.pc = self.fetch16(bus);
                        self.cycles += 10;
                    }
                    1 => self.exec_cb(bus, pfx),
                    2 => {
                        let n = self.fetch8(bus);
                        bus.port_out(n, self.a);
                        self.cycles += 11;
                    }
                    3 => {
                        let n = self.fetch8(bus);
                        self.a = bus.port_in(n);
                        self.cycles += 11;
                    }
                    4 => {
                        let sp = self.sp;
                        let v = self.read16(bus, sp);
                        let hl = self.idx(pfx);
                        self.write16(bus, sp, hl);
                        self.set_idx(pfx, v);
                        self.cycles += 19;
                    }
                    5 => {
                        let de = self.de();
                        let hl = self.hl();
                        self.set_de(hl);
                        self.set_hl(de);
                        self.cycles += 4;
                    }
                    6 => {
                        self.iff1 = false;
                        self.iff2 = false;
                        self.cycles += 4;
                    }
                    _ => {
                        self.iff1 = true;
                        self.iff2 = true;
                        self.ei_delay = true;
                        self.cycles += 4;
                    }
                },
                4 => {
                    let nn = self.fetch16(bus);
                    if self.cond(y) {
                        let pc = self.pc;
                        self.push16(bus, pc);
                        self.pc = nn;
                        self.cycles += 17;
                    } else {
                        self.cycles += 10;
                    }
                }
                5 => {
                    if q == 0 {
                        let v = if p == 3 { self.af() } else { self.rp(p, pfx) };
                        self.push16(bus, v);
                        self.cycles += 11;
                    } else if p == 0 {
                        let nn = self.fetch16(bus);
                        let pc = self.pc;
                        self.push16(bus, pc);
                        self.pc = nn;
                        self.cycles += 17;
                    } else if p == 2 {
                        self.exec_ed(bus);
                    } else {
                        // DD/FD are consumed in step(); unreachable
                        self.cycles += 4;
                    }
                }
                6 => {
                    let n = self.fetch8(bus);
                    self.alu(y, n);
                    self.cycles += 7;
                }
                _ => {
                    let pc = self.pc;
                    self.push16(bus, pc);
                    self.pc = (y as u16) * 8;
                    self.cycles += 11;
                }
            },
        }
    }

    fn exec_cb(&mut self, bus: &mut impl Bus, pfx: u8) {
        if pfx == 0 {
            let op = self.fetch_op(bus);
            let (x, y, z) = (op >> 6, (op >> 3) & 7, op & 7);
            if z == 6 {
                let addr = self.hl();
                let v = bus.read(addr);
                let r = self.cb_op(x, y, v);
                if x != 1 {
                    bus.write(addr, r);
                }
                self.cycles += if x == 1 { 12 } else { 15 };
            } else {
                let v = self.get8(z, 0);
                let r = self.cb_op(x, y, v);
                if x != 1 {
                    self.set8(z, 0, r);
                }
                self.cycles += 8;
            }
        } else {
            let d = self.fetch8(bus) as i8;
            let op = self.fetch8(bus);
            let (x, y, z) = (op >> 6, (op >> 3) & 7, op & 7);
            let addr = self.idx(pfx).wrapping_add(d as i16 as u16);
            let v = bus.read(addr);
            let r = self.cb_op(x, y, v);
            if x != 1 {
                bus.write(addr, r);
                if z != 6 {
                    self.set8(z, 0, r);
                }
            }
            self.cycles += if x == 1 { 16 } else { 19 };
        }
    }

    fn exec_ed(&mut self, bus: &mut impl Bus) {
        let op = self.fetch_op(bus);
        let (x, y, z) = (op >> 6, (op >> 3) & 7, op & 7);
        let (p, q) = (y >> 1, y & 1);
        match x {
            1 => match z {
                0 => {
                    let v = bus.port_in(self.c);
                    if y != 6 {
                        self.set8(y, 0, v);
                    }
                    self.f = (self.f & C) | sz(v) | parity(v);
                    self.cycles += 12;
                }
                1 => {
                    let v = if y == 6 { 0 } else { self.get8(y, 0) };
                    bus.port_out(self.c, v);
                    self.cycles += 12;
                }
                2 => {
                    let hl = self.hl();
                    let rr = self.rp(p, 0);
                    let r = if q == 0 { self.sbc16(hl, rr) } else { self.adc16(hl, rr) };
                    self.set_hl(r);
                    self.cycles += 15;
                }
                3 => {
                    let nn = self.fetch16(bus);
                    if q == 0 {
                        let v = self.rp(p, 0);
                        self.write16(bus, nn, v);
                    } else {
                        let v = self.read16(bus, nn);
                        self.set_rp(p, 0, v);
                    }
                    self.cycles += 20;
                }
                4 => {
                    let v = self.a;
                    self.a = 0;
                    self.alu(2, v);
                    self.cycles += 8;
                }
                5 => {
                    self.pc = self.pop16(bus);
                    self.iff1 = self.iff2;
                    self.cycles += 14;
                }
                6 => {
                    self.im = match y & 3 {
                        0 | 1 => 0,
                        2 => 1,
                        _ => 2,
                    };
                    self.cycles += 8;
                }
                _ => match y {
                    0 => {
                        self.i = self.a;
                        self.cycles += 9;
                    }
                    1 => {
                        self.r = self.a;
                        self.cycles += 9;
                    }
                    2 | 3 => {
                        self.a = if y == 2 { self.i } else { self.r };
                        self.f = (self.f & C) | sz(self.a) | if self.iff2 { PV } else { 0 };
                        self.cycles += 9;
                    }
                    4 => {
                        let hl = self.hl();
                        let m = bus.read(hl);
                        bus.write(hl, (self.a << 4) | (m >> 4));
                        self.a = (self.a & 0xF0) | (m & 0xF);
                        self.f = (self.f & C) | sz(self.a) | parity(self.a);
                        self.cycles += 18;
                    }
                    5 => {
                        let hl = self.hl();
                        let m = bus.read(hl);
                        bus.write(hl, (m << 4) | (self.a & 0xF));
                        self.a = (self.a & 0xF0) | (m >> 4);
                        self.f = (self.f & C) | sz(self.a) | parity(self.a);
                        self.cycles += 18;
                    }
                    _ => self.cycles += 8,
                },
            },
            2 if y >= 4 && z < 4 => {
                let dec = y & 1 == 1;
                let rep = y & 2 != 0;
                let step = |v: u16| if dec { v.wrapping_sub(1) } else { v.wrapping_add(1) };
                let again;
                match z {
                    0 => {
                        let v = bus.read(self.hl());
                        bus.write(self.de(), v);
                        let (hl, de) = (step(self.hl()), step(self.de()));
                        self.set_hl(hl);
                        self.set_de(de);
                        let bc = self.bc().wrapping_sub(1);
                        self.set_bc(bc);
                        let n = v.wrapping_add(self.a);
                        self.f = (self.f & (S | Z | C))
                            | if bc != 0 { PV } else { 0 }
                            | (n & X)
                            | if n & 2 != 0 { Y } else { 0 };
                        again = rep && bc != 0;
                    }
                    1 => {
                        let v = bus.read(self.hl());
                        let res = self.a.wrapping_sub(v);
                        let h = (self.a & 0xF) < (v & 0xF);
                        let hl = step(self.hl());
                        self.set_hl(hl);
                        let bc = self.bc().wrapping_sub(1);
                        self.set_bc(bc);
                        let n = res.wrapping_sub(h as u8);
                        self.f = (self.f & C)
                            | (res & S)
                            | if res == 0 { Z } else { 0 }
                            | if h { H } else { 0 }
                            | N
                            | if bc != 0 { PV } else { 0 }
                            | (n & X)
                            | if n & 2 != 0 { Y } else { 0 };
                        again = rep && bc != 0 && res != 0;
                    }
                    2 => {
                        let v = bus.port_in(self.c);
                        bus.write(self.hl(), v);
                        self.b = self.b.wrapping_sub(1);
                        let hl = step(self.hl());
                        self.set_hl(hl);
                        self.f = (self.f & C) | sz(self.b) | N;
                        again = rep && self.b != 0;
                    }
                    _ => {
                        let v = bus.read(self.hl());
                        self.b = self.b.wrapping_sub(1);
                        bus.port_out(self.c, v);
                        let hl = step(self.hl());
                        self.set_hl(hl);
                        self.f = (self.f & C) | sz(self.b) | N;
                        again = rep && self.b != 0;
                    }
                }
                if again {
                    self.pc = self.pc.wrapping_sub(2);
                    self.cycles += 21;
                } else {
                    self.cycles += 16;
                }
            }
            _ => self.cycles += 8,
        }
    }
}
