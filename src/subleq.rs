pub const SUBLEQ_RAM_LEN: usize = 65536;

pub struct SubLeq {
    pub ram: [i16; SUBLEQ_RAM_LEN],
    pub pc: u16,
}

impl SubLeq {
    pub fn new() -> Self {
        Self {
            ram: [0; SUBLEQ_RAM_LEN],
            pc: 0,
        }
    }

    pub fn clock(&mut self) {
        let a = self.ram[self.pc as usize] as u16 as usize;
        let b = self.ram[self.pc.wrapping_add(1) as usize] as u16 as usize;
        let c = self.ram[self.pc.wrapping_add(2) as usize] as u16;

        self.ram[b] = self.ram[b].wrapping_sub(self.ram[a]);
        self.pc = (self.pc + 3 ^ c) & ((self.ram[b] > 0) as u16).wrapping_neg() ^ c;
    }
}
