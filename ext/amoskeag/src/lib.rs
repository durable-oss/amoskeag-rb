use std::collections::HashMap;
use std::str::FromStr;
use magnus::{Error, RHash, RString, RArray, Symbol, Value, define_module, function, prelude::*, TryConvert, IntoValue};
use amoskeag::{compile, evaluate, CompiledProgram};
use amoskeag_stdlib_operators::{Decimal, Value as AmoskeagValue};

// Maximum allowed sizes for defensive programming
const MAX_SOURCE_SIZE: usize = 10 * 1024 * 1024; // 10MB
const MAX_SYMBOLS_COUNT: usize = 10_000;
const MAX_DICT_DEPTH: usize = 100;

#[magnus::wrap(class = "Amoskeag::Program", free_immediately, size)]
struct Program {
    program: CompiledProgram,
}

impl Program {
    fn new(program: CompiledProgram) -> Self {
        Self { program }
    }
}

// Build a Ruby BigDecimal from an Amoskeag Decimal.
//
// BigDecimal is constructed from the decimal's string form rather than an f64
// so the exact scale survives the crossing: 19.99d stays 19.99, not
// 19.989999999999998. This mirrors what the upstream Ruby transpiler emits.
fn decimal_to_ruby(d: &Decimal) -> Result<Value, Error> {
    // Always called from a Ruby thread (we are inside a Ruby method call), so
    // this cannot fail in practice; map the error rather than unwrap so a
    // misuse surfaces as a Ruby exception instead of a panic across the FFI.
    let ruby = magnus::Ruby::get().map_err(|e| {
        Error::new(magnus::exception::runtime_error(), e.to_string())
    })?;

    // bigdecimal ships with Ruby but is not loaded by default, and the
    // extension cannot assume the host program required it.
    ruby.require("bigdecimal")?;

    // Kernel#BigDecimal, the documented constructor. Decimal's Display
    // preserves trailing zeros, which is the precision the program asked for.
    ruby.module_kernel()
        .funcall("BigDecimal", (d.to_string(),))
}

// Convert Amoskeag Value to Ruby Value
fn amoskeag_value_to_ruby(value: &AmoskeagValue) -> Result<Value, Error> {
    match value {
        AmoskeagValue::Number(n) => Ok(n.into_value()),
        // Exact base-10 maps to BigDecimal; mapping it to Float would discard
        // exactly the guarantee the `d` suffix exists to provide.
        AmoskeagValue::Decimal(d) => decimal_to_ruby(d),
        // Exact integers stay Ruby Integers rather than widening to Float.
        AmoskeagValue::Integer(i) => Ok(i.into_value()),
        AmoskeagValue::String(s) => Ok(RString::new(s).into_value()),
        AmoskeagValue::Boolean(b) => Ok(b.into_value()),
        AmoskeagValue::Nil => Ok(().into_value()),
        AmoskeagValue::Array(arr) => {
            let ruby_arr = RArray::new();
            for item in arr {
                ruby_arr.push(amoskeag_value_to_ruby(item)?)?;
            }
            Ok(ruby_arr.into_value())
        }
        AmoskeagValue::Dictionary(dict) => {
            let ruby_hash = RHash::new();
            for (k, v) in dict.iter() {
                ruby_hash.aset(k.as_str(), amoskeag_value_to_ruby(v)?)?;
            }
            Ok(ruby_hash.into_value())
        }
        AmoskeagValue::Symbol(s) => {
            // Wrap symbols in a hash with "__symbol__" key
            let ruby_hash = RHash::new();
            ruby_hash.aset("__symbol__", s.as_str())?;
            Ok(ruby_hash.into_value())
        }
    }
}

// Is this value a BigDecimal?
//
// Checked via the Object constant rather than a cached class handle because
// bigdecimal may not be loaded at all. If the constant is undefined, nothing in
// the process can be a BigDecimal, so the answer is no without loading it.
fn is_big_decimal(value: Value) -> Result<bool, Error> {
    let object = magnus::Ruby::get_with(value).class_object();

    // magnus 0.7 exposes only const_get, which raises on a missing constant,
    // so ask Ruby directly. `false` for the inherit flag keeps this to
    // top-level constants.
    let defined: bool = object.funcall("const_defined?", ("BigDecimal", false))?;
    if !defined {
        return Ok(false);
    }

    let class: Value = object.const_get("BigDecimal")?;
    match magnus::RClass::from_value(class) {
        Some(class) => Ok(value.is_kind_of(class)),
        None => Ok(false),
    }
}

// Convert Ruby Value to Amoskeag Value with depth tracking
fn ruby_value_to_amoskeag_with_depth(value: Value, depth: usize) -> Result<AmoskeagValue, Error> {
    // Defensive: Prevent stack overflow from deeply nested structures
    if depth > MAX_DICT_DEPTH {
        return Err(Error::new(
            magnus::exception::arg_error(),
            format!("Data structure too deeply nested (max depth: {})", MAX_DICT_DEPTH)
        ));
    }

    // Check for nil first (most specific)
    if value.is_nil() {
        return Ok(AmoskeagValue::Nil);
    }

    // Check for composite types (Hash and Array) before primitive types
    // This prevents bool::try_convert from converting hashes to true
    if let Some(hash) = RHash::from_value(value) {
        // Defensive: Check hash size
        let len = hash.len();
        if len > 100_000 {
            return Err(Error::new(
                magnus::exception::arg_error(),
                format!("Hash too large: {} keys (max: 100,000)", len)
            ));
        }

        let mut result = HashMap::new();

        // Collect entries first to avoid borrowing issues
        let mut entries = Vec::new();
        hash.foreach(|key: Value, val: Value| {
            entries.push((key, val));
            Ok(magnus::r_hash::ForEach::Continue)
        })?;

        for (key, val) in entries {
            // Defensive: Validate key is a string or symbol
            let key_str = if let Some(s) = RString::from_value(key) {
                unsafe { s.as_str() }?.to_string()
            } else if let Some(sym) = Symbol::from_value(key) {
                sym.name()?.to_string()
            } else {
                return Err(Error::new(
                    magnus::exception::arg_error(),
                    "Hash key must be String or Symbol"
                ));
            };

            // Defensive: Validate key length
            if key_str.len() > 1000 {
                return Err(Error::new(
                    magnus::exception::arg_error(),
                    format!("Hash key too long: {} bytes (max: 1000)", key_str.len())
                ));
            }

            result.insert(key_str, ruby_value_to_amoskeag_with_depth(val, depth + 1)?);
        }
        return Ok(AmoskeagValue::Dictionary(result));
    }

    if let Some(arr) = RArray::from_value(value) {
        // Defensive: Check array size
        let len = arr.len();
        if len > 1_000_000 {
            return Err(Error::new(
                magnus::exception::arg_error(),
                format!("Array too large: {} elements (max: 1,000,000)", len)
            ));
        }

        let mut result = Vec::new();
        for item in arr.into_iter() {
            result.push(ruby_value_to_amoskeag_with_depth(item, depth + 1)?);
        }
        return Ok(AmoskeagValue::Array(result));
    }

    // Now check primitive types.
    //
    // Exact types are checked before f64 so they keep their exactness. Going
    // through f64 first would silently turn a Ruby Integer into a binary float
    // and a BigDecimal into an approximation, which is the error the upstream
    // Decimal/Integer variants exist to prevent.
    if value.is_kind_of(magnus::class::integer()) {
        // i64::try_convert raises RangeError for bignums that do not fit.
        // Amoskeag's Integer is i64, so fall through to Number for those: an
        // f64 approximation of a huge count is better than a hard failure.
        if let Ok(i) = i64::try_convert(value) {
            return Ok(AmoskeagValue::Integer(i));
        }
    }

    if is_big_decimal(value)? {
        // BigDecimal#to_s uses engineering notation ("0.1999e2") by default;
        // to_s("F") gives the plain form rust_decimal can parse.
        let s: String = value.funcall("to_s", ("F",))?;
        let d = Decimal::from_str(&s).map_err(|e| {
            Error::new(
                magnus::exception::arg_error(),
                format!("BigDecimal value cannot be represented exactly: {} ({})", s, e)
            )
        })?;
        return Ok(AmoskeagValue::Decimal(d));
    }

    if let Ok(n) = f64::try_convert(value) {
        // Defensive: Check for special float values
        if !n.is_finite() {
            return Err(Error::new(
                magnus::exception::arg_error(),
                format!("Invalid number: {} (must be finite)", n)
            ));
        }
        Ok(AmoskeagValue::Number(n))
    } else if let Ok(s) = String::try_convert(value) {
        // Defensive: Check string length
        if s.len() > 100 * 1024 * 1024 {
            return Err(Error::new(
                magnus::exception::arg_error(),
                format!("String too large: {} bytes (max: 100MB)", s.len())
            ));
        }
        Ok(AmoskeagValue::String(s))
    } else if let Some(sym) = Symbol::from_value(value) {
        let name = sym.name()?.to_string();
        // Defensive: Validate symbol name
        if name.is_empty() {
            return Err(Error::new(
                magnus::exception::arg_error(),
                "Symbol name cannot be empty"
            ));
        }
        if name.len() > 1000 {
            return Err(Error::new(
                magnus::exception::arg_error(),
                format!("Symbol name too long: {} bytes (max: 1000)", name.len())
            ));
        }
        // Convert Ruby symbol to Amoskeag symbol directly
        Ok(AmoskeagValue::Symbol(name))
    } else if let Ok(b) = bool::try_convert(value) {
        // Check for bool AFTER all other types to avoid converting hashes/arrays to true
        Ok(AmoskeagValue::Boolean(b))
    } else {
        Err(Error::new(
            magnus::exception::arg_error(),
            format!("Unsupported type for conversion: {}", unsafe { value.classname() })
        ))
    }
}

fn ruby_value_to_amoskeag(value: Value) -> Result<AmoskeagValue, Error> {
    ruby_value_to_amoskeag_with_depth(value, 0)
}

// Amoskeag.compile(source, symbols = nil) -> Program
fn amoskeag_compile(args: &[Value]) -> Result<Program, Error> {
    let args = magnus::scan_args::scan_args::<(Value,), (Option<Value>,), (), (), (), ()>(args)?;
    let (source_val,) = args.required;
    let (symbols_val,) = args.optional;

    // Validate source argument
    if source_val.is_nil() {
        return Err(Error::new(
            magnus::exception::arg_error(),
            "source must be a String, got NilClass"
        ));
    }

    let source = RString::from_value(source_val).ok_or_else(|| {
        Error::new(
            magnus::exception::arg_error(),
            format!("source must be a String, got {}", unsafe { source_val.classname() })
        )
    })?;

    // Defensive: Validate source
    let source_str = unsafe { source.as_str() }?;
    if source_str.is_empty() {
        return Err(Error::new(
            magnus::exception::arg_error(),
            "source cannot be empty"
        ));
    }
    if source_str.len() > MAX_SOURCE_SIZE {
        return Err(Error::new(
            magnus::exception::arg_error(),
            format!("source too large: {} bytes (max: {})", source_str.len(), MAX_SOURCE_SIZE)
        ));
    }

    let symbol_vec: Vec<String> = if let Some(syms_val) = symbols_val {
        if syms_val.is_nil() {
            Vec::new()
        } else {
            let syms = RArray::from_value(syms_val).ok_or_else(|| {
                Error::new(
                    magnus::exception::arg_error(),
                    format!("symbols must be an Array, got {}", unsafe { syms_val.classname() })
                )
            })?;

            // Defensive: Check symbols array size
            let len = syms.len();
            if len > MAX_SYMBOLS_COUNT {
                return Err(Error::new(
                    magnus::exception::arg_error(),
                    format!("Too many symbols: {} (max: {})", len, MAX_SYMBOLS_COUNT)
                ));
            }

            let mut result = Vec::new();
            for val in syms.into_iter() {
                let sym_str = if let Some(s) = RString::from_value(val) {
                    unsafe { s.as_str() }?.to_string()
                } else if let Some(sym) = Symbol::from_value(val) {
                    sym.name()?.to_string()
                } else {
                    return Err(Error::new(
                        magnus::exception::arg_error(),
                        format!("symbols must contain only Strings or Symbols, got {}", unsafe { val.classname() })
                    ));
                };

                // Defensive: Validate symbol
                if sym_str.is_empty() {
                    return Err(Error::new(
                        magnus::exception::arg_error(),
                        "Symbol cannot be empty"
                    ));
                }
                if sym_str.len() > 1000 {
                    return Err(Error::new(
                        magnus::exception::arg_error(),
                        format!("Symbol too long: {} bytes (max: 1000)", sym_str.len())
                    ));
                }

                result.push(sym_str);
            }
            result
        }
    } else {
        Vec::new()
    };

    let symbols_refs: Vec<&str> = symbol_vec.iter().map(|s| s.as_str()).collect();

    let program = compile(source_str, &symbols_refs).map_err(|e| {
        Error::new(
            get_compile_error_class().unwrap_or_else(|_| magnus::exception::runtime_error()),
            format!("{:?}", e)
        )
    })?;

    Ok(Program::new(program))
}

// Amoskeag.evaluate(program, data) -> Object
fn amoskeag_evaluate(args: &[Value]) -> Result<Value, Error> {
    let args = magnus::scan_args::scan_args::<(Value, Value), (), (), (), (), ()>(args)?;
    let (program_val, data_val) = args.required;

    // Validate program argument
    if program_val.is_nil() {
        return Err(Error::new(
            magnus::exception::arg_error(),
            "program must be an Amoskeag::Program, got NilClass"
        ));
    }

    let program: &Program = match <&Program>::try_convert(program_val) {
        Ok(p) => p,
        Err(_) => {
            return Err(Error::new(
                magnus::exception::arg_error(),
                format!("program must be an Amoskeag::Program, got {}", unsafe { program_val.classname() })
            ));
        }
    };

    // Validate data argument
    if data_val.is_nil() {
        return Err(Error::new(
            magnus::exception::arg_error(),
            "data must be a Hash, got NilClass"
        ));
    }

    let data = RHash::from_value(data_val).ok_or_else(|| {
        Error::new(
            magnus::exception::arg_error(),
            format!("data must be a Hash, got {}", unsafe { data_val.classname() })
        )
    })?;

    // Defensive: Validate data
    let data_len = data.len();
    if data_len > 100_000 {
        return Err(Error::new(
            magnus::exception::arg_error(),
            format!("data hash too large: {} keys (max: 100,000)", data_len)
        ));
    }

    // Convert Ruby hash to HashMap
    let mut data_map = HashMap::new();

    // Collect entries first to avoid borrowing issues
    let mut entries = Vec::new();
    data.foreach(|key: Value, val: Value| {
        entries.push((key, val));
        Ok(magnus::r_hash::ForEach::Continue)
    })?;

    for (key, val) in entries {
        let key_str = if let Some(s) = RString::from_value(key) {
            unsafe { s.as_str() }?.to_string()
        } else if let Some(sym) = Symbol::from_value(key) {
            sym.name()?.to_string()
        } else {
            return Err(Error::new(
                magnus::exception::arg_error(),
                "data keys must be Strings or Symbols"
            ));
        };

        data_map.insert(key_str, ruby_value_to_amoskeag(val)?);
    }

    let result = evaluate(&program.program, &data_map).map_err(|e| {
        Error::new(
            get_eval_error_class().unwrap_or_else(|_| magnus::exception::runtime_error()),
            format!("{:?}", e)
        )
    })?;

    amoskeag_value_to_ruby(&result)
}

// Amoskeag.eval_expression(source, data, symbols = nil) -> Object
fn amoskeag_eval_expression(args: &[Value]) -> Result<Value, Error> {
    let args = magnus::scan_args::scan_args::<(Value, Value), (Option<Value>,), (), (), (), ()>(args)?;
    let (source_val, data_val) = args.required;
    let (symbols_val,) = args.optional;

    // Validate source
    if source_val.is_nil() {
        return Err(Error::new(
            magnus::exception::arg_error(),
            "source must be a String, got NilClass"
        ));
    }

    // Validate data
    if data_val.is_nil() {
        return Err(Error::new(
            magnus::exception::arg_error(),
            "data must be a Hash, got NilClass"
        ));
    }

    let compile_args = if let Some(syms) = symbols_val {
        vec![source_val, syms]
    } else {
        vec![source_val]
    };

    let program = amoskeag_compile(&compile_args)?;

    let program_val: Value = program.into_value();
    let eval_args = vec![program_val, data_val];
    amoskeag_evaluate(&eval_args)
}

// Get error classes from module constants
fn get_compile_error_class() -> Result<magnus::ExceptionClass, Error> {
    let module = define_module("Amoskeag")?;
    let class: magnus::Value = module.const_get("CompileError")?;
    Ok(magnus::ExceptionClass::from_value(class).unwrap())
}

fn get_eval_error_class() -> Result<magnus::ExceptionClass, Error> {
    let module = define_module("Amoskeag")?;
    let class: magnus::Value = module.const_get("EvalError")?;
    Ok(magnus::ExceptionClass::from_value(class).unwrap())
}

#[magnus::init]
fn init() -> Result<(), Error> {
    let module = define_module("Amoskeag")?;

    // Define base Error class
    let error_class = module.define_error("Error", magnus::exception::standard_error())?;

    // Define error subclasses inheriting from Amoskeag::Error
    module.define_error("CompileError", error_class)?;
    module.define_error("EvalError", error_class)?;

    // Define the Program class - make it non-allocatable from Ruby
    let program_class = module.define_class("Program", magnus::class::object())?;
    program_class.undef_default_alloc_func();

    module.define_module_function("compile", function!(amoskeag_compile, -1))?;
    module.define_module_function("evaluate", function!(amoskeag_evaluate, -1))?;
    module.define_module_function("eval_expression", function!(amoskeag_eval_expression, -1))?;

    Ok(())
}

// These tests embed a Ruby VM in the test binary, which requires the `embed`
// feature (see Cargo.toml):
//
//     cargo test --features embed
//
// Without it the module is compiled out rather than failing to build, so a
// plain `cargo test` still checks the rest of the crate.
#[cfg(all(test, feature = "embed"))]
mod tests {
    use super::*;

    // magnus::Ruby::init takes a closure and may be called only once per
    // process, so every case runs inside a single VM instead of each test
    // booting its own. Rust would otherwise run these on separate threads,
    // which a single embedded VM does not support.
    #[test]
    fn conversion_tests() {
        magnus::Ruby::init(|ruby| {
            hash_converts_to_dictionary_not_boolean(ruby);
            array_converts_to_array_not_boolean(ruby);
            true_converts_to_boolean_true(ruby);
            false_converts_to_boolean_false(ruby);
            nil_converts_to_nil(ruby);
            float_converts_to_number(ruby);
            integer_converts_to_integer(ruby);
            big_integer_falls_back_to_number(ruby);
            big_decimal_converts_to_decimal(ruby);
            string_converts_to_string(ruby);
            symbol_converts_to_symbol(ruby);
            nested_hash_converts_correctly(ruby);
            empty_hash_converts_to_empty_dictionary(ruby);
            empty_array_converts_to_empty_array(ruby);
            decimal_converts_to_big_decimal(ruby);
            integer_converts_to_ruby_integer(ruby);
            Ok(())
        })
        .unwrap();
    }

    fn hash_converts_to_dictionary_not_boolean(ruby: &magnus::Ruby) {
        // Create a Ruby hash: {"a" => 1, "b" => 2}
        let hash: Value = ruby.eval(r#"{"a" => 1, "b" => 2}"#).unwrap();

        let result = ruby_value_to_amoskeag(hash).unwrap();

        // Should be a Dictionary, not Boolean(true)
        match result {
            AmoskeagValue::Dictionary(map) => {
                assert_eq!(map.len(), 2);
                // Ruby Integers map to exact Integers, not f64.
                assert_eq!(map.get("a"), Some(&AmoskeagValue::Integer(1)));
                assert_eq!(map.get("b"), Some(&AmoskeagValue::Integer(2)));
            }
            other => panic!("Expected Dictionary, got {:?}", other),
        }
    }

    fn array_converts_to_array_not_boolean(ruby: &magnus::Ruby) {
        let array: Value = ruby.eval("[1, 2, 3]").unwrap();

        let result = ruby_value_to_amoskeag(array).unwrap();

        match result {
            AmoskeagValue::Array(arr) => {
                assert_eq!(arr.len(), 3);
                assert_eq!(arr[0], AmoskeagValue::Integer(1));
                assert_eq!(arr[1], AmoskeagValue::Integer(2));
                assert_eq!(arr[2], AmoskeagValue::Integer(3));
            }
            other => panic!("Expected Array, got {:?}", other),
        }
    }

    fn true_converts_to_boolean_true(ruby: &magnus::Ruby) {
        let val: Value = ruby.eval("true").unwrap();
        assert_eq!(
            ruby_value_to_amoskeag(val).unwrap(),
            AmoskeagValue::Boolean(true)
        );
    }

    fn false_converts_to_boolean_false(ruby: &magnus::Ruby) {
        let val: Value = ruby.eval("false").unwrap();
        assert_eq!(
            ruby_value_to_amoskeag(val).unwrap(),
            AmoskeagValue::Boolean(false)
        );
    }

    fn nil_converts_to_nil(ruby: &magnus::Ruby) {
        let val: Value = ruby.eval("nil").unwrap();
        assert_eq!(ruby_value_to_amoskeag(val).unwrap(), AmoskeagValue::Nil);
    }

    fn float_converts_to_number(ruby: &magnus::Ruby) {
        let val: Value = ruby.eval("42.5").unwrap();
        assert_eq!(
            ruby_value_to_amoskeag(val).unwrap(),
            AmoskeagValue::Number(42.5)
        );
    }

    fn integer_converts_to_integer(ruby: &magnus::Ruby) {
        // Past 2**53 an f64 round-trip would corrupt the value, which is the
        // reason integers no longer go through Number.
        let val: Value = ruby.eval("9007199254740993").unwrap();
        assert_eq!(
            ruby_value_to_amoskeag(val).unwrap(),
            AmoskeagValue::Integer(9_007_199_254_740_993)
        );
    }

    fn big_integer_falls_back_to_number(ruby: &magnus::Ruby) {
        // Bignums beyond i64 cannot be exact; approximating beats failing.
        let val: Value = ruby.eval("2 ** 80").unwrap();
        match ruby_value_to_amoskeag(val).unwrap() {
            AmoskeagValue::Number(n) => assert!((n - 2f64.powi(80)).abs() < 1e9),
            other => panic!("Expected Number, got {:?}", other),
        }
    }

    fn big_decimal_converts_to_decimal(ruby: &magnus::Ruby) {
        let val: Value = ruby
            .eval(r#"require "bigdecimal"; BigDecimal("19.99")"#)
            .unwrap();

        match ruby_value_to_amoskeag(val).unwrap() {
            AmoskeagValue::Decimal(d) => assert_eq!(d.to_string(), "19.99"),
            other => panic!("Expected Decimal, got {:?}", other),
        }
    }

    fn string_converts_to_string(ruby: &magnus::Ruby) {
        let val: Value = ruby.eval(r#""hello""#).unwrap();
        assert_eq!(
            ruby_value_to_amoskeag(val).unwrap(),
            AmoskeagValue::String("hello".to_string())
        );
    }

    fn symbol_converts_to_symbol(ruby: &magnus::Ruby) {
        let val: Value = ruby.eval(":test").unwrap();
        assert_eq!(
            ruby_value_to_amoskeag(val).unwrap(),
            AmoskeagValue::Symbol("test".to_string())
        );
    }

    fn nested_hash_converts_correctly(ruby: &magnus::Ruby) {
        let hash: Value = ruby
            .eval(r#"{"user" => {"name" => "Alice", "age" => 30}}"#)
            .unwrap();

        match ruby_value_to_amoskeag(hash).unwrap() {
            AmoskeagValue::Dictionary(map) => match map.get("user") {
                Some(AmoskeagValue::Dictionary(inner)) => {
                    assert_eq!(
                        inner.get("name"),
                        Some(&AmoskeagValue::String("Alice".to_string()))
                    );
                    assert_eq!(inner.get("age"), Some(&AmoskeagValue::Integer(30)));
                }
                other => panic!("Expected nested Dictionary, got {:?}", other),
            },
            other => panic!("Expected Dictionary, got {:?}", other),
        }
    }

    fn empty_hash_converts_to_empty_dictionary(ruby: &magnus::Ruby) {
        let hash: Value = ruby.eval("{}").unwrap();

        match ruby_value_to_amoskeag(hash).unwrap() {
            AmoskeagValue::Dictionary(map) => assert_eq!(map.len(), 0),
            other => panic!("Expected empty Dictionary, got {:?}", other),
        }
    }

    fn empty_array_converts_to_empty_array(ruby: &magnus::Ruby) {
        let array: Value = ruby.eval("[]").unwrap();

        match ruby_value_to_amoskeag(array).unwrap() {
            AmoskeagValue::Array(arr) => assert_eq!(arr.len(), 0),
            other => panic!("Expected empty Array, got {:?}", other),
        }
    }

    // Outbound direction: the two new variants must reach Ruby intact.
    fn decimal_converts_to_big_decimal(ruby: &magnus::Ruby) {
        let d = Decimal::from_str("19.99").unwrap();
        let value = amoskeag_value_to_ruby(&AmoskeagValue::Decimal(d)).unwrap();

        let is_big_decimal: bool = ruby
            .class_object()
            .const_get::<_, magnus::RClass>("BigDecimal")
            .map(|c| value.is_kind_of(c))
            .unwrap();
        assert!(is_big_decimal, "Decimal should map to BigDecimal");

        // to_s("F") gives the plain form; the default is engineering notation.
        let s: String = value.funcall("to_s", ("F",)).unwrap();
        assert_eq!(s, "19.99");
    }

    fn integer_converts_to_ruby_integer(ruby: &magnus::Ruby) {
        let value = amoskeag_value_to_ruby(&AmoskeagValue::Integer(9_007_199_254_740_993)).unwrap();

        assert!(
            value.is_kind_of(ruby.class_integer()),
            "Integer should stay a Ruby Integer"
        );
        assert_eq!(i64::try_convert(value).unwrap(), 9_007_199_254_740_993);
    }
}
