use lazy_static::lazy_static;
use std::collections::HashSet;
use std::fmt::{Error, Write};

//use cmsis_pack::pdsc::Device;
use cmsis_pack::pdsc::sequence::*;

lazy_static! {
    static ref DEBUG_ACCESS_VARIABLES: HashSet<&'static str> = {
        let mut m = HashSet::new();
        m.insert("__protocol");
        m.insert("__connection");
        m.insert("__dp");
        m.insert("__ap");
        m.insert("__apid");
        m.insert("__traceout");
        m.insert("__errorcontrol");
        m.insert("__FlashOp");
        m.insert("__FlashAddr");
        m.insert("__FlashLen");
        m.insert("__FlashArg");
        m
    };
}

struct Writer {
    buf: String,
    indent: usize,
    start: bool,
}

impl Writer {
    fn new() -> Self {
        Writer {
            buf: String::new(),
            indent: 0,
            start: true,
        }
    }

    fn indent(&mut self) -> &mut Self {
        self.indent += 1;
        self
    }

    fn outdent(&mut self) -> &mut Self {
        self.indent -= 1;
        self
    }

    fn end(self) -> String {
        self.buf
    }
}

impl Write for Writer {
    fn write_str(&mut self, s: &str) -> Result<(), Error> {
        for c in s.chars() {
            if c == '\n' {
                self.buf.push('\n');
                self.start = true;
                continue;
            }

            if self.start {
                for _ in 0..self.indent {
                    write!(self.buf, "    ")?;
                }

                self.start = false;
            }

            self.buf.push(c);
        }

        Ok(())
    }
}

pub fn gen_sequences(sequences: &Sequences) -> Result<String, Error> {
    let mut w = Writer::new();

    /*writeln!(w, "function context()")?;
    w.indent();
    writeln!(w, "ctx = probe_context()")?;
    //w.indent();
    for (name, var) in &dev.debug_vars.0 {
        //write!(w, "{} = ", var.name)?;
        //gen_num(&mut w, var.value, var.value_style)?;
        //writeln!(w, ",")?;
        write!(w, "ctx.{} = ", name)?;
        gen_num(&mut w, var.0, var.1)?;
        writeln!(w)?;
    }
    //w.outdent();
    writeln!(w, "return ctx")?;
    w.outdent();
    writeln!(w, "end")?;
    writeln!(w)?;*/

    for (name, seq) in &sequences.0 {
        writeln!(w, "fn s_{}(ctx) {{", name)?;
        w.indent();

        writeln!(w, "let __Result = 0;")?;

        let mut locals = HashSet::new();
        locals.insert("__Result".to_string());
        gen_body(&mut w, &seq.body, &mut locals)?;
        writeln!(w, "__Result")?;

        w.outdent();
        writeln!(w, "}}")?;
        writeln!(w)?;
    }

    Ok(w.end())
}

fn gen_body(w: &mut Writer, body: &[Struct], locals: &mut HashSet<String>) -> Result<(), Error> {
    for s in body {
        match s {
            Struct::Control(ctrl) => {
                if let Some(if_cond) = &ctrl.if_cond {
                    writeln!(w, "if {} {{", expr_as(Type::Bool, if_cond, locals, false)?)?;
                    w.indent();
                }

                if let Some(info) = &ctrl.info {
                    writeln!(w, "ctx.info(\"{}\");", info)?;
                }

                match &ctrl.while_cond {
                    Some(Expr::Num(Num(n, _))) if *n > 0 => {
                        // pure delay loop

                        writeln!(w, "ctx.sleep({});", ctrl.timeout.unwrap_or(0))?;
                    },
                    Some(while_cond) => {
                        // while loop

                        if let Some(timeout) = ctrl.timeout {
                            writeln!(w, "{{")?;
                            w.indent();
                            writeln!(w, "let timeout = ctx.timeout({});", timeout)?;
                        }

                        writeln!(w, "while {} {{", expr_as(Type::Bool, while_cond, locals, false)?)?;
                        w.indent();

                        gen_body(w, &ctrl.body, &mut locals.clone())?;

                        if ctrl.timeout.is_some() {
                            writeln!(w, "timeout.check();")?;
                        }

                        w.outdent();
                        writeln!(w, "}}")?;

                        if ctrl.timeout.is_some() {
                            w.outdent();
                            writeln!(w, "}}")?;
                        }
                    },
                    _ => {
                        // no loop

                        gen_body(w, &ctrl.body, &mut locals.clone())?;
                    },
                }

                if ctrl.if_cond.is_some() {
                    w.outdent();
                    writeln!(w, "}}")?;
                }
            },
            Struct::Block(block) => {
                if let Some(info) = &block.info {
                    writeln!(w, "ctx.info(\"{}\");", info)?;
                }

                if block.atomic {
                    writeln!(w, "// atomic")?;
                    writeln!(w, "{{")?;
                    w.indent();
                }

                for stmt in &block.body {
                    gen_stmt(w, stmt, locals)?;
                }

                if block.atomic {
                    w.outdent();
                    writeln!(w, "}}")?;
                }
            },
        }
    }

    Ok(())
}

fn var_path(name: &str, locals: &HashSet<String>) -> String {
    if DEBUG_ACCESS_VARIABLES.contains(name) {
        format!("ctx.cmsis{}", name)
    } else if locals.contains(name) {
        format!("l_{}", name)
    } else {
        format!("ctx.vars[\"{}\"]", name)
    }
}

fn gen_stmt(w: &mut Writer, stmt: &Stmt, locals: &mut HashSet<String>) -> Result<(), Error> {
    match stmt {
        Stmt::Declare(var, expr) => {
            locals.insert(var.to_string());

            writeln!(w, "let l_{} = {};", var, expr_as(Type::Num, expr, locals, false)?)?;
        },
        Stmt::Assign(var, expr) => {
            writeln!(w, "{} = {};",
                var_path(var, locals),
                expr_as(Type::Num, expr, locals, false)?)?;
        },
        Stmt::Expr(expr) => {
            gen_expr(w, expr, locals, false)?;
            writeln!(w, ";")?;
        }
    }

    Ok(())
}

fn expr_as(result_type: Type, expr: &Expr, locals: &mut HashSet<String>, nested: bool) -> Result<String, Error> {
    let mut w = Writer::new();
    let t = gen_expr(&mut w, expr, locals, nested)?;
    let s = w.end();

    if t == result_type {
        Ok(s)
    } else {
        Ok(match result_type {
            Type::Bool => format!("bool({})", s),
            Type::Num => format!("u64({})", s),
        })
    }
}

fn gen_expr(w: &mut Writer, expr: &Expr, locals: &mut HashSet<String>, nested: bool) -> Result<Type, Error> {
    use Type::*;

    Ok(match expr {
        Expr::Num(num) => {
            gen_num(w, *num)?;

            Num
        }
        Expr::Var(var) => {
            write!(w, "{}", var_path(var, locals))?;

            Num
        },
        Expr::Call(func, args) if func == "Sequence" => {
            match args.as_slice() {
                &[Arg::String(ref name)] => write!(w, "s_{}(ctx)", name)?,
                _ => write!(w, " #ERROR# ")?,
            }

            Num
        },
        Expr::Call(func, args) => {
            write!(w, "ctx.cmsis_{}(", func)?;

            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    write!(w, ", ")?;
                }

                match a {
                    Arg::Expr(expr) => { gen_expr(w, expr, locals, false)?; },
                    Arg::String(s) => { write!(w, "\"{}\"", s)?; },
                }
            }

            write!(w, ")")?;

            Num
        },
        Expr::Unary(UnOp::Not, operand) => {
            write!(w, "!{}", expr_as(Bool, operand, locals, true)?)?;

            Bool
        },
        Expr::Unary(op, operand) => {
            let op = match op {
                UnOp::BitNot => "!",
                UnOp::Pos => "",
                UnOp::Neg => "-",
                _ => unreachable!(),
            };

            if nested {
                write!(w, "(")?;
            }

            write!(w, "{}", op)?;
            gen_expr(w, operand, locals, true)?;

            if nested {
                write!(w, ")")?;
            }

            Num
        },
        Expr::Binary(op, operands) => {
            use Style::*;

            enum Style {
                Plain,
                Cmp,
                Logic,
            }

            let (op, style) = match op {
                BinOp::Add => ("+", Plain),
                BinOp::Sub => ("-", Plain),
                BinOp::Mul => ("*", Plain),
                BinOp::Div => ("/", Plain),
                BinOp::Rem => ("%", Plain),
                BinOp::BitXor => ("^", Plain),
                BinOp::BitAnd => ("&", Plain),
                BinOp::BitOr => ("|", Plain),
                BinOp::Shr => (">>", Plain),
                BinOp::Shl => ("<<", Plain),
                BinOp::Lt => ("<", Cmp),
                BinOp::Le => ("<=", Cmp),
                BinOp::Ge => (">=", Cmp),
                BinOp::Gt => (">", Cmp),
                BinOp::Eq => ("==", Cmp),
                BinOp::Ne => ("!=", Cmp),
                BinOp::And => ("&&", Logic),
                BinOp::Or => ("||", Logic),
            };

            if nested {
                write!(w, "(")?;
            }

            let t = match style {
                Plain | Cmp => {
                    gen_expr(w, &operands.0, locals, true)?;
                    write!(w, " {} ", op)?;
                    gen_expr(w, &operands.1, locals, true)?;

                    match style {
                        Cmp => Bool,
                        _ => Num,
                    }
                },
                Logic => {
                    write!(
                        w,
                        "{} {} {}",
                        expr_as(Bool, &operands.0, locals, true)?,
                        op,
                        expr_as(Bool, &operands.0, locals, true)?)?;

                    Bool
                },
            };

            if nested {
                write!(w, ")")?;
            }

            t
        },
        Expr::Cond(operands) => {
            write!(w, "if {} {{", expr_as(Bool, &operands.0, locals, false)?)?;
            gen_expr(w, &operands.1, locals, false)?;
            write!(w, " }} else {{ ")?;
            gen_expr(w, &operands.2, locals, false)?;
            write!(w, " }}")?;

            Num
        },
    })
}

#[derive(Eq, PartialEq)]
enum Type {
    Num,
    Bool,
}

fn gen_num(w: &mut Writer, num: Num) -> Result<(), Error> {
    if num.0 >= 0x8000_0000_0000_0000 {
        let high = (num.0 >> 32) as u32;
        let low = num.0 as u32;

        write!(w, "u64_big(0x{:08x}, 0x{:08x})", high, low)?;
    } else {
        match num.1 {
            NumStyle::Dec => {
                write!(w, "u64({})", num.0)?;
            },
            NumStyle::Hex => {
                write!(w, "u64(0x{:x})", num.0)?;
            },
        }
    }

    Ok(())
}

/*lazy_static! {
    static ref UNOPS:
}*/