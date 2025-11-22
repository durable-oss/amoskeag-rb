# frozen_string_literal: true

require "test_helper"

class WrapperTest < Minitest::Test
  include TestHelper

  # Test module structure and constants

  def test_module_is_defined
    assert defined?(Amoskeag), "Amoskeag module should be defined"
    assert_kind_of Module, Amoskeag, "Amoskeag should be a module"
  end

  def test_error_class_hierarchy
    assert defined?(Amoskeag::Error), "Amoskeag::Error should be defined"
    assert_kind_of Class, Amoskeag::Error, "Amoskeag::Error should be a class"
    assert Amoskeag::Error < StandardError, "Amoskeag::Error should inherit from StandardError"
  end

  def test_compile_error_class
    assert defined?(Amoskeag::CompileError), "Amoskeag::CompileError should be defined"
    assert_kind_of Class, Amoskeag::CompileError, "Amoskeag::CompileError should be a class"
    assert Amoskeag::CompileError < StandardError, "Amoskeag::CompileError should inherit from StandardError"
    # CompileError is defined in the native extension, so it should exist
  end

  def test_eval_error_class
    assert defined?(Amoskeag::EvalError), "Amoskeag::EvalError should be defined"
    assert_kind_of Class, Amoskeag::EvalError, "Amoskeag::EvalError should be a class"
    assert Amoskeag::EvalError < StandardError, "Amoskeag::EvalError should inherit from StandardError"
    # EvalError is defined in the native extension, so it should exist
  end

  def test_compile_error_inheritance
    # Test that compile errors are instances of the right classes
    error = assert_compile_error("if true") # Missing end
    assert_instance_of Amoskeag::CompileError, error
    assert error.is_a?(StandardError)
  end

  def test_eval_error_inheritance
    # Test that eval errors are instances of the right classes
    program = Amoskeag.compile("x / y", [])
    error = assert_eval_error(program, { "x" => 10, "y" => 0 }) # Division by zero
    assert_instance_of Amoskeag::EvalError, error
    assert error.is_a?(StandardError)
  end

  def test_program_class_structure
    assert defined?(Amoskeag::Program), "Amoskeag::Program should be defined"
    assert_kind_of Class, Amoskeag::Program, "Amoskeag::Program should be a class"

    # Should not be instantiable directly
    assert_raises(NoMethodError) do
      Amoskeag::Program.new
    end

    # But should be instantiable through compile
    program = Amoskeag.compile("2 + 2", [])
    assert_instance_of Amoskeag::Program, program
  end

  def test_program_immutability
    program1 = Amoskeag.compile("2 + 2", [])
    program2 = Amoskeag.compile("2 + 2", [])

    # Programs with same source should be different objects
    refute_same program1, program2

    # Programs should not have dangerous methods
    # (This is more of a safety check - the native implementation should ensure this)
    assert program1.respond_to?(:inspect) # Basic Object methods should work
    assert program1.respond_to?(:to_s)
  end

  def test_module_methods_are_defined
    assert Amoskeag.respond_to?(:compile), "Amoskeag.compile should be defined"
    assert Amoskeag.respond_to?(:evaluate), "Amoskeag.evaluate should be defined"
    assert Amoskeag.respond_to?(:eval_expression), "Amoskeag.eval_expression should be defined"
  end

  def test_native_methods_are_aliased
    # The native methods should be available as aliases
    assert Amoskeag.respond_to?(:compile_native), "compile_native should be aliased"
    assert Amoskeag.respond_to?(:evaluate_native), "evaluate_native should be aliased"
    assert Amoskeag.respond_to?(:eval_expression_native), "eval_expression_native should be aliased"
  end

  def test_version_constant
    assert defined?(Amoskeag::VERSION), "VERSION constant should be defined"
    assert_kind_of String, Amoskeag::VERSION, "VERSION should be a string"
    refute_empty Amoskeag::VERSION, "VERSION should not be empty"
    assert Amoskeag::VERSION.frozen?, "VERSION should be frozen"
  end

  # Test data validation edge cases

  def test_validate_data_accepts_json_serializable_types
    # These should all pass validation
    valid_data = {
      "string" => "hello",
      "number" => 42.5,
      "integer" => 42,
      "boolean_true" => true,
      "boolean_false" => false,
      "nil_value" => nil,
      "symbol" => :test,
      "array" => [1, "two", true, nil],
      "hash" => { "nested" => "value" }
    }

    program = Amoskeag.compile("x", [])
    # Should not raise an error
    result = Amoskeag.evaluate(program, { "x" => valid_data })
    # Note: evaluate transforms data (integers to floats, symbols to special format)
    expected = {
      "string" => "hello",
      "number" => 42.5,
      "integer" => 42.0, # Integer becomes Float
      "boolean_true" => true,
      "boolean_false" => false,
      "nil_value" => nil,
      "symbol" => { "__symbol__" => "test" }, # Symbol becomes hash
      "array" => [1.0, "two", true, nil], # Integer in array becomes Float
      "hash" => { "nested" => "value" }
    }
    assert_equal expected, result
  end

  def test_validate_data_rejects_invalid_object_types
    program = Amoskeag.compile("x", [])

    # Test various invalid types
    invalid_objects = [
      Object.new,
      Class.new,
      Module.new,
      -> { "lambda" },
      /regex/,
      Complex(1, 2),
      Range.new(1, 10)
    ]

    invalid_objects.each do |obj|
      assert_argument_error(/invalid type/) do
        Amoskeag.evaluate(program, { "x" => obj })
      end
    end

    # Time might be accepted if it has to_json, but let's test it
    # Time.now should be rejected as it's not in our allowed types
    assert_argument_error(/invalid type/) do
      Amoskeag.evaluate(program, { "x" => Time.now })
    end
  end

  def test_validate_data_rejects_invalid_hash_keys
    program = Amoskeag.compile("x", [])

    # Integer keys
    assert_argument_error(/key must be String or Symbol/) do
      Amoskeag.evaluate(program, { 123 => "value" })
    end

    # Object keys
    assert_argument_error(/key must be String or Symbol/) do
      Amoskeag.evaluate(program, { Object.new => "value" })
    end

    # Array keys
    assert_argument_error(/key must be String or Symbol/) do
      Amoskeag.evaluate(program, { [] => "value" })
    end
  end

  def test_validate_data_accepts_symbol_and_string_keys
    program = Amoskeag.compile("x.a + x.b", [])

    # String keys
    result = Amoskeag.evaluate(program, { "x" => { "a" => 1, "b" => 2 } })
    assert_equal 3.0, result

    # Symbol keys
    result = Amoskeag.evaluate(program, { "x" => { a: 1, b: 2 } })
    assert_equal 3.0, result

    # Mixed keys
    result = Amoskeag.evaluate(program, { "x" => { "a" => 1, b: 2 } })
    assert_equal 3.0, result
  end

  def test_validate_data_rejects_infinite_numbers
    program = Amoskeag.compile("x", [])

    assert_argument_error(/must be finite/) do
      Amoskeag.evaluate(program, { "x" => Float::INFINITY })
    end

    assert_argument_error(/must be finite/) do
      Amoskeag.evaluate(program, { "x" => -Float::INFINITY })
    end
  end

  def test_validate_data_rejects_nan
    program = Amoskeag.compile("x", [])

    assert_argument_error(/must be finite/) do
      Amoskeag.evaluate(program, { "x" => Float::NAN })
    end
  end

  def test_validate_data_accepts_finite_numbers
    program = Amoskeag.compile("x + y + z", [])

    # Various finite numbers
    data = {
      "x" => 42,
      "y" => 3.14159,
      "z" => -123.456
    }

    result = Amoskeag.evaluate(program, data)
    assert_in_delta 42 + 3.14159 - 123.456, result, 0.0001
  end

  def test_validate_data_rejects_bigdecimal
    require "bigdecimal"
    program = Amoskeag.compile("x", [])

    # BigDecimal should be rejected as it's not Integer or Float
    big_num = BigDecimal("123.456789")
    assert_argument_error(/invalid type/) do
      Amoskeag.evaluate(program, { "x" => big_num })
    end
  end

  def test_validate_data_rejects_rational
    program = Amoskeag.compile("x", [])

    # Rational should be rejected as it's not Integer or Float
    rational = Rational(3, 4)
    assert_argument_error(/invalid type/) do
      Amoskeag.evaluate(program, { "x" => rational })
    end
  end

  def test_validate_data_nested_validation
    program = Amoskeag.compile("x", [])

    # Deeply nested valid structure
    nested = {
      "level1" => {
        "level2" => {
          "level3" => [
            "string",
            42,
            true,
            nil,
            { "deep" => :symbol }
          ]
        }
      }
    }

    result = Amoskeag.evaluate(program, { "x" => nested })
    # Data is transformed: integers to floats, symbols to special format
    expected = {
      "level1" => {
        "level2" => {
          "level3" => [
            "string",
            42.0,
            true,
            nil,
            { "deep" => { "__symbol__" => "symbol" } }
          ]
        }
      }
    }
    assert_equal expected, result
  end

  def test_validate_data_error_messages_include_path
    program = Amoskeag.compile("x", [])

    # Error should include path information
    error = assert_argument_error do
      Amoskeag.evaluate(program, { "data" => { "invalid" => Object.new } })
    end
    assert_match(/data\[.*invalid.*\]/, error.message)
  end

  # Test symbol handling in compile/eval_expression

  def test_compile_symbol_conversion
    # Symbols should be converted to strings
    program = Amoskeag.compile("if x :yes else :no end", [:yes, :no])
    result = Amoskeag.evaluate(program, { "x" => true })
    assert_equal :yes, result
  end

  def test_eval_expression_symbol_conversion
    # Symbols should be converted to strings
    result = Amoskeag.eval_expression("if x :yes else :no end", { "x" => true }, [:yes, :no])
    assert_equal :yes, result
  end

  def test_compile_mixed_symbol_types
    # Mix of strings and symbols should work
    program = Amoskeag.compile("if x :success else :failure end", ["success", :failure])
    result = Amoskeag.evaluate(program, { "x" => true })
    assert_equal :success, result
  end

  def test_compile_duplicate_symbols_allowed
    # Duplicate symbols should be allowed (native code handles deduplication)
    program = Amoskeag.compile(":symbol", [:symbol, :symbol])
    result = Amoskeag.evaluate(program, {})
    assert_equal :symbol, result
  end

  # Test method aliasing works correctly

  def test_native_methods_still_work
    # The native methods should still be callable
    program = Amoskeag.compile_native("2 + 2", [])
    result = Amoskeag.evaluate_native(program, {})
    assert_equal 4.0, result
  end

  def test_eval_expression_native_method
    result = Amoskeag.eval_expression_native("2 + 2", {}, nil)
    assert_equal 4.0, result
  end

  # Test thread safety of wrapper

  def test_wrapper_thread_safety_compile
    threads = []
    results = []

    10.times do |i|
      threads << Thread.new do
        program = Amoskeag.compile("#{i} + 1", [])
        result = Amoskeag.evaluate(program, {})
        results << result
      end
    end

    threads.each(&:join)
    assert_equal 10, results.size
    results.each_with_index do |result, i|
      assert_equal (i + 1).to_f, result
    end
  end

  def test_wrapper_thread_safety_eval_expression
    threads = []
    results = []

    10.times do |i|
      threads << Thread.new do
        result = Amoskeag.eval_expression("#{i} + 1", {})
        results << result
      end
    end

    threads.each(&:join)
    assert_equal 10, results.size
    results.each_with_index do |result, i|
      assert_equal (i + 1).to_f, result
    end
  end

  # Test memory management

  def test_program_garbage_collection
    # Create many programs and ensure no memory leaks
    programs = []
    100.times do |i|
      programs << Amoskeag.compile("#{i} + 1", [])
    end

    # Force garbage collection
    GC.start

    # Programs should still work
    programs.each_with_index do |program, i|
      result = Amoskeag.evaluate(program, {})
      assert_equal (i + 1).to_f, result
    end
  end

  def test_data_validation_garbage_collection
    program = Amoskeag.compile("x", [])

    # Create large data structures
    10.times do
      large_data = { "array" => Array.new(1000, "test") }
      result = Amoskeag.evaluate(program, { "x" => large_data })
      assert_equal large_data, result
    end

    GC.start
  end
end