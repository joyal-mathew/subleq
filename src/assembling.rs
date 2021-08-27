use {
    crate::subleq::{ SUBLEQ_RAM_LEN, SubLeq },
    std::{
        result,
        collections::HashMap,
        io::{ self, Write, Read },
        fs,
    },
};

macro_rules! mapfns {
    ($($arg: ident $size: literal),*) => {{
        let mut map = HashMap::new();

        $(
            map.insert(stringify!($arg), ($size, Self::$arg as fn(&mut Self, &[&str]) -> Result<()>));
        )*

        map
    }}
}

macro_rules! build_registers {
    ($($arg: ident),*) => {
        impl Registers {
            build_registers!(@ 0, $($arg),*);
        }
    };

    (@ $i: expr, $head: ident, $($arg: ident),*) => {
        pub const $head: i16 = INITIAL_OFFSET as i16 + $i;
        build_registers!(@ $i + 1, $($arg),*);
    };

    (@ $i: expr, $head: ident) => {
        pub const $head: i16 = INITIAL_OFFSET as i16 + $i;
        pub const DATA_OFFSET: i16 = INITIAL_OFFSET as i16 + $i + 1;
    }
}

macro_rules! register_registers {
    ($a: ident, $($arg: ident),*) => {
        register_registers!(@ 0, $a, $($arg),*);
    };

    (@ $i: expr, $a:ident, $head: ident, $($arg: ident),*) => {
        $a.vars.insert(stringify!($head).to_owned(), Registers::$head);
        register_registers!(@ $i + 1, $a, $($arg),*);
    };

    (@ $i: expr, $a: ident, $head: ident) => {
        $a.vars.insert(stringify!($head).to_owned(), Registers::$head);
        $a.vars.insert("DATA_OFFSET".to_owned(), Registers::DATA_OFFSET);
    }
}

type Result<T> = result::Result<T, String>;

pub struct SubLeqSystem {
    subleq: Box<SubLeq>,
    var_offset: usize,
    fin_offset: usize,
    brk: bool,
    debug: bool,
    instructions: HashMap<u16, String>,
}

impl SubLeqSystem {
    pub fn run(&mut self) {
        while !self.brk {
            if self.debug {
                io::stdin().read(&mut [0]).unwrap();
                self.debug(32);
                io::stdout().flush().unwrap();
                io::stdin().read(&mut [0]).unwrap();
            }

            let out_addr = self.subleq.ram[self.subleq.pc as usize + 1];
            self.subleq.clock();
            let out_data = self.subleq.ram[out_addr as u16 as usize].wrapping_neg();

            match out_addr {
                Registers::BRK => self.brk = true,
                Registers::IOUT => {
                    print!("{}", out_data);
                    self.subleq.ram[out_addr as usize] = 0;
                }
                Registers::COUT => {
                    print!("{}", std::char::from_u32(out_data as u32).unwrap_or('�'));
                    self.subleq.ram[out_addr as usize] = 0;
                }
                _ => (),
            }
        }
    }

    fn debug(&self, len: usize) {
        let o = Registers::DATA_OFFSET as usize + self.var_offset;
        let pc = self.subleq.pc as usize;

        print!("pc: {} ({}) | ", self.instructions.get(&self.subleq.pc).unwrap_or(&"???".to_owned()), pc);

        for w in &self.subleq.ram[INITIAL_OFFSET as usize..o] {
            print!("{} ", w);
        }
        println!();

        println!("({}\t{}\t{})", self.subleq.ram[pc], self.subleq.ram[pc + 1], self.subleq.ram[pc + 2]);

        for w in &self.subleq.ram[..len] {
            print!("{} ", w);
        }
        println!();
    }
}

struct Assembler {
    line: usize,
    offset: u16,
    data: [i16; SUBLEQ_RAM_LEN],
    vars: HashMap<String, i16>,
    instructions: HashMap<&'static str, (u16, fn(&mut Self, args: &[&str]) -> Result<()>)>,
}

pub const INITIAL_OFFSET: u16 = 0x100;

struct Registers;
build_registers!(BRK, IOUT, COUT, P1, N1, O, J, K, A, B, S, R, A1, A2, X, Y, F);

pub fn assemble<'a>(mut program: String, debug: bool) -> Result<SubLeqSystem> {
    let mut slqsys = SubLeqSystem {
        subleq: Box::new(SubLeq::new()),
        var_offset: 0,
        fin_offset: 0,
        brk: false,
        debug,
        instructions: HashMap::new(),
    };

    let mut assembler = Assembler::new();

    register_registers!(assembler, BRK, IOUT, COUT, P1, N1, O, J, A, B, S, R, A1, A2, X, Y, F);

    assembler.data[Registers::P1 as usize] = 1;
    assembler.data[Registers::N1 as usize] = -1;
    assembler.data[Registers::O as usize] = -0x8000;
    assembler.data[Registers::J as usize] = 12;
    assembler.data[Registers::K as usize] = 15;

    let mut vars_count = 0;
    let mut includes = Vec::new();

    for line in program.split('\n') {
        if line.starts_with('%') {
            includes.push(line.trim().to_owned() + ".sla");
        }
    }

    for s in includes {
        program += &fs::read_to_string(&s[1..]).or(Err(format!("Could not include {}", &s[1..])))?[..];
    }

    let lines: Vec<&str> = program.split('\n').collect();

    for line in &lines {
        let args: Vec<&str> = line.split_ascii_whitespace().collect();

        match args.get(0).map(|a| a.bytes().next()).flatten() {
            Some(b'.') => match args[0] {
                ".dat" => {
                    let o = assembler.offset as i16;
                    let args = &args[1..];
                    vars_count += 1;
                    assembler.write(assembler.args_get(args, 1)?.parse().or_else(|_| Err(format!("Invalid integer {} (l {}, a 1)", args[1], assembler.line)))?);
                    match assembler.vars.insert(assembler.args_get(args, 0)?.to_string(), o) {
                        Some(_) => Err(format!("Redefined variable (l {})", assembler.line)),
                        None => Ok(())
                    }?
                }
                _ => return Err(format!("Undefined directive (l {})", assembler.line)),
            },
            _ => assembler.line += 1,
        }
    }

    assembler.line = 1;
    let mut o = assembler.offset;
    slqsys.subleq.pc = o;

    for line in &lines {
        let args: Vec<&str> = line.split_ascii_whitespace().collect();

        match args.get(0).map(|a| a.bytes().next()).flatten() {
            Some(b'#') => assembler.assemble_label(args[0], o)?,
            Some(b'.') | Some(b'%') | None => assembler.line += 1,
            Some(_) => {
                assembler.line += 1;
                o += 3 * assembler.instructions.get(args[0]).ok_or_else(|| format!("Undefined instruction (l {})", assembler.line - 1))?.0;
            }
        }
    }

    assembler.line = 1;

    for line in &lines {
        let args: Vec<&str> = line.split_ascii_whitespace().collect();

        match args.get(0).map(|a| a.bytes().next()).flatten() {
            Some(b'#') | Some(b'.') | Some(b'%') | None => assembler.line += 1,
            Some(_) => {
                slqsys.instructions.insert(assembler.offset, args[0].to_owned());
                assembler.assemble_instruction(args[0], &args[1..])?
            },
        }
    }

    slqsys.subleq.ram = assembler.data;
    slqsys.var_offset = vars_count;
    slqsys.fin_offset = assembler.offset as usize;

    Ok(slqsys)
}

impl Assembler {
    fn new() -> Self {
        Self {
            line: 1,
            offset: Registers::DATA_OFFSET as u16,
            data: [0; SUBLEQ_RAM_LEN],
            vars: HashMap::new(),
            instructions: mapfns!(
                set 0x4, slt 0x5, clr 0x1,
                neg 0x6, add 0x3, sub 0x1,
                beq 0x6, blq 0x2, bgq 0x4, slq 0x1,
                inc 0x1, dec 0x1,
                jmp 0x1, jsr 0xF, cll 0x11, ret 0x8
            ),
        }
    }

    fn assemble_label(&mut self, label: &str, offset: u16) -> Result<()> {
        self.line += 1;

        match self.vars.insert(label.to_owned(), offset as i16) {
            Some(_) => Err(format!("Redefined label (l {})", self.line - 1)),
            None => Ok(())
        }
    }

    fn assemble_instruction(&mut self, cmd: &str, args: &[&str]) -> Result<()> {
        self.line += 1;
        self.instructions.get(cmd).ok_or_else(|| format!("Undefined instruction (l {})", self.line - 1))?.1(self, args)
    }

    fn write(&mut self, word: i16) {
        self.data[self.offset as usize] = word;
        self.offset += 1;
    }

    fn pc_jump(&mut self, n: i16) {
        self.write(self.offset as i16 + 1 + 3 * n)
    }

    fn pc_offset(&mut self, n: i16) {
        self.write(self.offset as i16 + n);
    }

    fn args_get<'a>(&self, args: &'a [&'a str], i: usize) -> Result<&&'a str> {
        args.get(i).ok_or_else(|| format!("Expected an argument (l {}, a {})", self.line, i))
    }

    fn var_get(&self, args: &[&str], i: usize) -> Result<i16> {
        let s = self.args_get(args, i)?;
        self.vars.get(*s).copied().or_else(|| s.parse().ok()).ok_or_else(|| format!("Expected defined variable or address, got {} (l {}, a {})", args[i], self.line, i))
    }
}

/**
 * TODO:
 * EOR      exclusive or
 * AND      and
 * ORA      or
 */

//instructions
impl Assembler {
    fn set(&mut self, args: &[&str]) -> Result<()> {
        let var0 = self.var_get(args, 0)?;
        let var1 = self.var_get(args, 1)?;

        self.write(var0);
        self.write(var0);
        self.pc_jump(0);

        self.write(Registers::A);
        self.write(Registers::A);
        self.pc_jump(0);

        self.write(var1);
        self.write(Registers::A);
        self.pc_jump(0);

        self.write(Registers::A);
        self.write(var0);
        self.pc_jump(0);

        Ok(())
    }

    fn slt(&mut self, args: &[&str]) -> Result<()> {
        let var0 = self.var_get(args, 0)?;
        let var1 = self.var_get(args, 1)?;

        self.write(var0);
        self.write(var0);
        self.pc_jump(0);

        self.write(Registers::A);
        self.write(Registers::A);
        self.pc_jump(1);

        self.write(-var1);
        self.write(0);
        self.write(0);

        self.pc_offset(-3);
        self.write(Registers::A);
        self.pc_jump(0);

        self.write(Registers::A);
        self.write(var0);
        self.pc_jump(0);

        Ok(())
    }

    fn clr(&mut self, args: &[&str]) -> Result<()> {
        let var0 = self.var_get(args, 0)?;

        self.write(var0);
        self.write(var0);
        self.pc_jump(0);

        Ok(())
    }

    fn neg(&mut self, args: &[&str]) -> Result<()> {
        let var0 = self.var_get(args, 0)?;

        self.write(Registers::A);
        self.write(Registers::A);
        self.pc_jump(0);

        self.write(Registers::B);
        self.write(Registers::B);
        self.pc_jump(0);

        self.write(var0);
        self.write(Registers::A);
        self.pc_jump(0);

        self.write(var0);
        self.write(var0);
        self.pc_jump(0);

        self.write(Registers::A);
        self.write(Registers::B);
        self.pc_jump(0);

        self.write(Registers::B);
        self.write(var0);
        self.pc_jump(0);

        Ok(())
    }

    fn add(&mut self, args: &[&str]) -> Result<()> {
        let var0 = self.var_get(args, 0)?;
        let var1 = self.var_get(args, 1)?;

        self.write(Registers::A);
        self.write(Registers::A);
        self.pc_jump(0);

        self.write(var1);
        self.write(Registers::A);
        self.pc_jump(0);

        self.write(Registers::A);
        self.write(var0);
        self.pc_jump(0);

        Ok(())
    }

    fn sub(&mut self, args: &[&str]) -> Result<()> {
        let var0 = self.var_get(args, 0)?;
        let var1 = self.var_get(args, 1)?;

        self.write(var1);
        self.write(var0);
        self.pc_jump(0);

        Ok(())
    }

    fn jmp(&mut self, args: &[&str]) -> Result<()> {
        let label0 = self.var_get(args, 0)? as i16;

        self.write(Registers::A);
        self.write(Registers::A);
        self.write(label0);

        Ok(())
    }

    fn beq(&mut self, args: &[&str]) -> Result<()> {
        let var0 = self.var_get(args, 0)?;
        let label1 = self.var_get(args, 1)? as i16;

        self.write(Registers::A);
        self.write(Registers::A);
        self.pc_jump(0);

        self.write(Registers::B);
        self.write(Registers::B);
        self.pc_jump(0);

        self.write(var0);
        self.write(Registers::A);
        self.pc_jump(0);

        self.write(Registers::B);
        self.write(Registers::A);
        self.pc_jump(1);

        self.write(Registers::B);
        self.write(Registers::B);
        self.pc_jump(1);

        self.write(Registers::B);
        self.write(var0);
        self.write(label1);

        Ok(())
    }

    fn blq(&mut self, args: &[&str]) -> Result<()> {
        let var0 = self.var_get(args, 0)?;
        let label1 = self.var_get(args, 1)? as i16;

        self.write(Registers::A);
        self.write(Registers::A);
        self.pc_jump(0);

        self.write(Registers::A);
        self.write(var0);
        self.write(label1);

        Ok(())
    }

    fn bgq(&mut self, args: &[&str]) -> Result<()> {
        let var0 = self.var_get(args, 0)?;
        let label1 = self.var_get(args, 1)? as i16;

        self.write(Registers::A);
        self.write(Registers::A);
        self.pc_jump(0);

        self.write(Registers::B);
        self.write(Registers::B);
        self.pc_jump(0);

        self.write(var0);
        self.write(Registers::B);
        self.pc_jump(0);

        self.write(Registers::A);
        self.write(Registers::B);
        self.write(label1);

        Ok(())
    }

    fn slq(&mut self, args: &[&str]) -> Result<()> {
        let var0 = self.var_get(args, 0)?;
        let var1 = self.var_get(args, 1)?;
        let label2 = self.var_get(args, 2)? as i16;

        self.write(var1);
        self.write(var0);
        self.write(label2);

        Ok(())
    }

    fn inc(&mut self, args: &[&str]) -> Result<()> {
        let var0 = self.var_get(args, 0)?;

        self.write(Registers::N1);
        self.write(var0);
        self.pc_jump(0);

        Ok(())
    }

    fn dec(&mut self, args: &[&str]) -> Result<()> {
        let var0 = self.var_get(args, 0)?;

        self.write(Registers::P1);
        self.write(var0);
        self.pc_jump(0);

        Ok(())
    }

    fn jsr(&mut self, args: &[&str]) -> Result<()> {
        let label0 = self.var_get(args, 0)? as i16;

        self.pc_offset(30);
        self.pc_offset(29);
        self.pc_jump(0);

        self.pc_offset(28);
        self.pc_offset(27);
        self.pc_jump(0);

        self.pc_offset(28);
        self.pc_offset(27);
        self.pc_jump(0);

        self.pc_offset(28);
        self.pc_offset(27);
        self.pc_jump(0);

        self.write(Registers::A);
        self.write(Registers::A);
        self.pc_jump(0);

        self.write(Registers::S);
        self.write(Registers::A);
        self.pc_jump(0);

        self.write(Registers::A);
        self.pc_offset(11);
        self.pc_jump(0);

        self.write(Registers::A);
        self.pc_offset(9);
        self.pc_jump(0);

        self.write(Registers::A);
        self.pc_offset(9);
        self.pc_jump(0);

        self.write(Registers::A);
        self.pc_offset(9);
        self.pc_jump(0);

        self.write(0);
        self.write(0);
        self.pc_jump(0);

        self.pc_offset(0);
        self.write(0);
        self.pc_jump(0);

        self.write(Registers::J);
        self.write(0);
        self.pc_jump(0);

        self.write(Registers::N1);
        self.write(Registers::S);
        self.pc_jump(0);

        self.write(Registers::A);
        self.write(Registers::A);
        self.write(label0);

        Ok(())
    }

    fn cll(&mut self, args: &[&str]) -> Result<()> {
        let var0 = self.var_get(args, 0)?;

        self.pc_offset(50);
        self.pc_offset(49);
        self.pc_jump(0);

        self.pc_offset(30);
        self.pc_offset(29);
        self.pc_jump(0);

        self.pc_offset(28);
        self.pc_offset(27);
        self.pc_jump(0);

        self.pc_offset(28);
        self.pc_offset(27);
        self.pc_jump(0);

        self.pc_offset(28);
        self.pc_offset(27);
        self.pc_jump(0);

        self.write(Registers::A);
        self.write(Registers::A);
        self.pc_jump(0);

        self.write(Registers::S);
        self.write(Registers::A);
        self.pc_jump(0);

        self.write(Registers::A);
        self.pc_offset(11);
        self.pc_jump(0);

        self.write(Registers::A);
        self.pc_offset(9);
        self.pc_jump(0);

        self.write(Registers::A);
        self.pc_offset(9);
        self.pc_jump(0);

        self.write(Registers::A);
        self.pc_offset(9);
        self.pc_jump(0);

        self.write(0);
        self.write(0);
        self.pc_jump(0);

        self.pc_offset(0);
        self.write(0);
        self.pc_jump(0);

        self.write(Registers::K);
        self.write(0);
        self.pc_jump(0);

        self.write(Registers::N1);
        self.write(Registers::S);
        self.pc_jump(0);

        self.write(var0);
        self.pc_offset(4);
        self.pc_jump(0);

        self.write(Registers::A);
        self.write(Registers::A);
        self.write(0);

        Ok(())
    }

    fn ret(&mut self, _: &[&str]) -> Result<()> {
        self.pc_offset(18);
        self.pc_offset(17);
        self.pc_jump(0);

        self.pc_offset(20);
        self.pc_offset(19);
        self.pc_jump(0);

        self.write(Registers::P1);
        self.write(Registers::S);
        self.pc_jump(0);

        self.write(Registers::A);
        self.write(Registers::A);
        self.pc_jump(0);

        self.write(Registers::S);
        self.write(Registers::A);
        self.pc_jump(0);

        self.write(Registers::A);
        self.pc_offset(2);
        self.pc_jump(0);

        self.write(0);
        self.pc_offset(4);
        self.pc_jump(0);

        self.write(Registers::A);
        self.write(Registers::A);
        self.write(0);

        Ok(())
    }
}
