use std::collections::HashMap;
use magnus::{Error, RHash, RString, RArray, Symbol, Value, define_module, function, prelude::*, TryConvert, IntoValue};
use amoskeag::{compile, evaluate, CompiledProgram};
use amoskeag_stdlib_operators::Value as AmoskeagValue;

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

// Convert Amoskeag Value to Ruby Value
fn amoskeag_value_to_ruby(value: &AmoskeagValue) -> Result<Value, Error> {
    match value {
        AmoskeagValue::Number(n) => Ok(n.into_value()),
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

    // Now check primitive types
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

    Ok(Program { program })
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

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to initialize Ruby for testing
    fn init_ruby() {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            magnus::Ruby::init().unwrap();
        });
    }

    #[test]
    fn test_ruby_hash_converts_to_dictionary_not_boolean() {
        init_ruby();

        // Create a Ruby hash: {"a" => 1, "b" => 2}
        let hash: Value = magnus::eval(r#"{"a" => 1, "b" => 2}"#).unwrap();

        let result = ruby_value_to_amoskeag(hash).unwrap();

        // Should be a Dictionary, not Boolean(true)
        match result {
            AmoskeagValue::Dictionary(map) => {
                assert_eq!(map.len(), 2);
                assert_eq!(map.get("a"), Some(&AmoskeagValue::Number(1.0)));
                assert_eq!(map.get("b"), Some(&AmoskeagValue::Number(2.0)));
            }
            other => panic!("Expected Dictionary, got {:?}", other),
        }
    }

    #[test]
    fn test_ruby_array_converts_to_array_not_boolean() {
        init_ruby();

        let array: Value = magnus::eval("[1, 2, 3]").unwrap();

        let result = ruby_value_to_amoskeag(array).unwrap();

        match result {
            AmoskeagValue::Array(arr) => {
                assert_eq!(arr.len(), 3);
                assert_eq!(arr[0], AmoskeagValue::Number(1.0));
                assert_eq!(arr[1], AmoskeagValue::Number(2.0));
                assert_eq!(arr[2], AmoskeagValue::Number(3.0));
            }
            other => panic!("Expected Array, got {:?}", other),
        }
    }

    #[test]
    fn test_ruby_true_converts_to_boolean_true() {
        init_ruby();

        let val: Value = magnus::eval("true").unwrap();

        let result = ruby_value_to_amoskeag(val).unwrap();

        assert_eq!(result, AmoskeagValue::Boolean(true));
    }

    #[test]
    fn test_ruby_false_converts_to_boolean_false() {
        init_ruby();

        let val: Value = magnus::eval("false").unwrap();

        let result = ruby_value_to_amoskeag(val).unwrap();

        assert_eq!(result, AmoskeagValue::Boolean(false));
    }

    #[test]
    fn test_ruby_nil_converts_to_nil() {
        init_ruby();

        let val: Value = magnus::eval("nil").unwrap();

        let result = ruby_value_to_amoskeag(val).unwrap();

        assert_eq!(result, AmoskeagValue::Nil);
    }

    #[test]
    fn test_ruby_number_converts_to_number() {
        init_ruby();

        let val: Value = magnus::eval("42.5").unwrap();

        let result = ruby_value_to_amoskeag(val).unwrap();

        assert_eq!(result, AmoskeagValue::Number(42.5));
    }

    #[test]
    fn test_ruby_string_converts_to_string() {
        init_ruby();

        let val: Value = magnus::eval(r#""hello""#).unwrap();

        let result = ruby_value_to_amoskeag(val).unwrap();

        assert_eq!(result, AmoskeagValue::String("hello".to_string()));
    }

    #[test]
    fn test_ruby_symbol_converts_to_symbol() {
        init_ruby();

        let val: Value = magnus::eval(":test").unwrap();

        let result = ruby_value_to_amoskeag(val).unwrap();

        assert_eq!(result, AmoskeagValue::Symbol("test".to_string()));
    }

    #[test]
    fn test_nested_hash_converts_correctly() {
        init_ruby();

        let hash: Value = magnus::eval(r#"{"user" => {"name" => "Alice", "age" => 30}}"#).unwrap();

        let result = ruby_value_to_amoskeag(hash).unwrap();

        match result {
            AmoskeagValue::Dictionary(map) => {
                match map.get("user") {
                    Some(AmoskeagValue::Dictionary(inner)) => {
                        assert_eq!(inner.get("name"), Some(&AmoskeagValue::String("Alice".to_string())));
                        assert_eq!(inner.get("age"), Some(&AmoskeagValue::Number(30.0)));
                    }
                    other => panic!("Expected nested Dictionary, got {:?}", other),
                }
            }
            other => panic!("Expected Dictionary, got {:?}", other),
        }
    }

    #[test]
    fn test_empty_hash_converts_to_empty_dictionary() {
        init_ruby();

        let hash: Value = magnus::eval("{}").unwrap();

        let result = ruby_value_to_amoskeag(hash).unwrap();

        match result {
            AmoskeagValue::Dictionary(map) => {
                assert_eq!(map.len(), 0);
            }
            other => panic!("Expected empty Dictionary, got {:?}", other),
        }
    }

    #[test]
    fn test_empty_array_converts_to_empty_array() {
        init_ruby();

        let array: Value = magnus::eval("[]").unwrap();

        let result = ruby_value_to_amoskeag(array).unwrap();

        match result {
            AmoskeagValue::Array(arr) => {
                assert_eq!(arr.len(), 0);
            }
            other => panic!("Expected empty Array, got {:?}", other),
        }
    }
}
