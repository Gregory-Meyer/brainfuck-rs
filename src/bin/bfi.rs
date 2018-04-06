#![feature(io)]

extern crate clap;

use std::error::Error;

pub struct InterpreterState<R, R2, W>
where R: std::io::Read, R2: std::io::Read, W: std::io::Write {
    data: Vec<u32>,
    pointer: usize,
    read_iter: std::io::Chars<R>,
    writer: W,
    input_iter: std::io::Chars<R2>,
    instructions: Vec<char>,
    instruction_pointer: usize,
}

fn is_usable(c: char) -> bool {
    return c == '>' || c == '<' || c == '+' || c == '-' || c == '.'
           || c == ',' || c == '[' || c == ']';
}

impl<R, R2, W> InterpreterState<R, R2, W>
where R: std::io::Read, R2: std::io::Read, W: std::io::Write {
    pub fn new(reader: R, writer: W, input_reader: R2)
    -> InterpreterState<R, R2, W> {
        InterpreterState { data: vec![0; 65536], pointer: 0,
                           read_iter: reader.chars(), writer,
                           input_iter: input_reader.chars(),
                           instructions: Vec::new(), instruction_pointer: 0 }
    }

    fn increment(&mut self) {
        self.pointer = self.pointer.wrapping_add(1);
    }

    fn decrement(&mut self) {
        self.pointer = self.pointer.wrapping_sub(1);
    }

    fn dereference(&self) -> u32 {
        if self.pointer >= self.data.len() {
            return 0;
        }

        self.data[self.pointer]
    }

    fn dereference_mut(&mut self) -> &mut u32 {
        while self.pointer >= self.data.len() {
            self.grow()
        }

        &mut self.data[self.pointer]
    }

    fn grow(&mut self) {
        let length = std::cmp::max(1, self.data.len());

        self.data.resize(length * 2, 0);
    }

    fn write(&mut self) {
        let to_write = match std::char::from_u32(self.dereference()) {
            Some(c) => c,
            None => {
                eprintln!("cannot print invalid UTF-8 codepoint");
                return;
            }
        };

        match write!(&mut self.writer, "{}", to_write) {
            Ok(_) => (),
            Err(e) => eprintln!("error while writing: {}", e.description()),
        }
    }

    fn read(&mut self) -> std::io::Result<()> {
        match self.input_iter.next() {
            Some(r) => match r {
                Ok(c) => *self.dereference_mut() = c as u32,
                Err(e) => match e {
                    std::io::CharsError::NotUtf8 => {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "buffer did not contain valid UTF-8")
                        );
                    }
                    std::io::CharsError::Other(o) => return Err(o),
                }
            }
            None => {
                return Err(std::io::Error::new(std::io::ErrorKind::Other,
                                               "no instructions in buffer"));
            }
        }

        Ok(())
    }

    fn jump_if_zero(&mut self) -> std::io::Result<()> {
        if self.dereference() != 0 {
            return Ok(())
        }

        match self.instructions[self.instruction_pointer + 1..]
            .iter()
            .cloned()
            .enumerate()
            .filter(|(_i, c)| *c == ']')
            .map(|(i, _c)| i)
            .next() {
            Some(i) => self.instruction_pointer += i,
            None => {
                while self.instructions[self.instructions.len() - 1] != ']' {
                    match self.read_file() {
                        Ok(_) => (),
                        Err(e) => return Err(e),
                    }
                }
            }
        }

        Ok(())
    }

    fn jump_if_nonzero(&mut self) {
        if self.dereference() == 0 {
            return
        }

        match self.instructions[..self.instruction_pointer]
            .iter()
            .rev()
            .cloned()
            .enumerate()
            .filter(|(_i, c)| *c == '[')
            .map(|(i, _c)| i)
            .next() {
            Some(i) => self.instruction_pointer -= i,
            None => {
                eprintln!("no matching '[' found!");
            }
        }
    }

    fn read_file(&mut self) -> std::io::Result<()> {
        match self.read_iter.next() {
            Some(maybe_char) => match maybe_char {
                Ok(c) => if is_usable(c) {
                    self.instructions.push(c)
                } else {
                    return self.read_file()
                }
                Err(e) => match e {
                    std::io::CharsError::NotUtf8 => {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "buffer did not contain valid UTF-8")
                        );
                    }
                    std::io::CharsError::Other(o) => return Err(o),
                }
            }
            None => return Err(std::io::Error::new(std::io::ErrorKind::Other,
                                               "no instructions in buffer")),
        }

        Ok(())
    }

    fn repl(&mut self) -> std::io::Result<()> {
        while self.instruction_pointer >= self.instructions.len() {
            match self.read_file() {
                Ok(_) => (),
                Err(e) => return Err(e),
            }
        }

        println!("p = {}, ip = {}, {:?}, {:?}",
                 self.pointer,
                 self.instruction_pointer,
                 self.instructions,
                 self.data);

        let instruction = self.instructions[self.instruction_pointer];

        match instruction {
            '>' => self.increment(),
            '<' => self.decrement(),
            '+' => {
                let deref = self.dereference();

                *self.dereference_mut() = deref.wrapping_add(1);
            }
            '-' => {
                let deref = self.dereference();

                *self.dereference_mut() = deref.wrapping_sub(1);
            }
            '.' => self.write(),
            ',' =>  match self.read() {
                Ok(_) => (),
                Err(e) => return Err(e),
            }
            '[' => match self.jump_if_zero() {
                Ok(_) => (),
                Err(e) => return Err(e),
            }
            ']' => self.jump_if_nonzero(),
            _ => (),
        }

        self.instruction_pointer += 1;

        Ok(())
    }
}

fn main() {
    let matches = clap::App::new("bfi")
        .version("0.1.0")
        .about("Brainfuck interpreter")
        .author("Gregory Meyer <gregjm@umich.edu>")
        .arg(clap::Arg::with_name("FILE")
             .required(true)
             .index(1))
        .get_matches();

    let filename = matches.value_of("FILE").unwrap();

    let file = match std::fs::File::open(filename) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("could not open file '{}': {}", filename,
                      e.description());
            std::process::exit(1);
        }
    };

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();

    let mut interpreter = InterpreterState::new(file, stdout.lock(),
                                                stdin.lock());

    while interpreter.repl().is_ok() { }
}
