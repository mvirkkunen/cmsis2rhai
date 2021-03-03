use std::convert::TryInto;
use std::fs::read_to_string;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};
//use std::rc::Rc;

use cmsis_pack::utils::FromElem;
use cmsis_pack::pdsc::{Package, sequence::Sequences};
use rhai::{Engine, Scope, Dynamic, EvalAltResult, RegisterFn, RegisterResultFn};
use rhai::packages::Package as _;
use simplelog::*;

mod generator;

fn main() {
    let logger = TermLogger::init(LevelFilter::Info, Config::default(), TerminalMode::Mixed);
    if logger.is_err() {
        eprintln!("Logging backend could not be initialized.");
    }

    let pdsc = read_to_string("/home/matti/dl/st401/Keil.STM32F4xx_DFP.pdsc").unwrap();
    let package = Package::from_string(&pdsc).unwrap();

    let defaults = Sequences::from_path(Path::new("default_sequences.xml")).unwrap();
    let defaults_rhai = generator::gen_sequences(&defaults).unwrap();
    println!("// default\n\n{}", defaults_rhai);

    let dev = package.devices.0.values().last().unwrap();
    let dev_rhai = generator::gen_sequences(&dev.sequences).unwrap();
    println!("\n// device\n\n{}", dev_rhai);

    let mut engine = Engine::new_raw();
    engine.set_max_expr_depths(128, 128);

    engine.load_package(rhai::packages::ArithmeticPackage::new().get());
    engine.load_package(rhai::packages::LogicPackage::new().get());
    engine.register_fn("bool", |val: u64| -> bool { val != 0 });
    //engine.register_fn("int", |val: bool| -> u64 { val.into() });
    engine.register_fn("u64", |val: bool| -> u64 { val.into() });
    engine.register_fn("u64", |val: i64| -> u64 { val as u64 });
    engine.register_fn("u64_big", |high: i64, low: i64| -> u64 { (high as u64) << 32 | (low as u64) });

    engine.register_type::<SequenceContext>()
        .register_fn("sleep", SequenceContext::sleep)
        .register_fn("timeout", SequenceContext::timeout)
        .register_fn("cmsis_Write32", SequenceContext::cmsis_Write32)
        .register_fn("cmsis_Read32", SequenceContext::cmsis_Read32)
        .register_fn("cmsis_DAP_SWJ_Pins", SequenceContext::cmsis_DAP_SWJ_Pins);

    engine.register_type::<Timeout>()
        .register_result_fn("check", Timeout::check);

    let mut scope = Scope::new();

    let ast = engine.compile(&defaults_rhai).unwrap()
        .merge(&engine.compile(&dev_rhai).unwrap());

    let ctx = SequenceContext;

    let _result: i64 = engine.call_fn(
        &mut scope,
        &ast,
        "s_ResetHardwareDeassert",
        (ctx,)).unwrap();
}

#[derive(Clone)]
struct SequenceContext;

#[allow(non_snake_case)]
impl SequenceContext {
    pub fn sleep(&mut self, timeout: i64) {
        thread::sleep(Duration::from_micros(timeout.try_into().unwrap()));
    }

    pub fn timeout(&mut self, timeout: i64) -> Timeout {
        Timeout(Instant::now() + Duration::from_micros(timeout.try_into().unwrap()))
    }

    pub fn cmsis_Write32(&mut self, addr: u64, val: u64) -> u64 {
        println!("writing {:x} to {:x}", val, addr);
        0
    }

    pub fn cmsis_Read32(&mut self, addr: u64) -> u64 {
        println!("reading {:x}", addr);
        0
    }

    pub fn cmsis_DAP_SWJ_Pins(&mut self, pinout: u64, pinselect: u64, pinwait: u64) -> u64 {
        //println!("DAP_SWJ_Pins {:08b} {:08b} {}", pinout, pinselect, pinwait);

        0
    }
}

#[derive(Clone)]
struct Timeout(Instant);

impl Timeout {
    pub fn check(&mut self) -> Result<Dynamic, Box<EvalAltResult>> {
        thread::sleep(Duration::from_millis(1));

        if Instant::now() > self.0 {
            Err("Timeout expired".into())
        } else {
            Ok(().into())
        }
    }
}
