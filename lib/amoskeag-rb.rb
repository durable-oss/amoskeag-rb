# frozen_string_literal: true

require 'json'
require_relative 'amoskeag-rb/version'

# Load the native extension
begin
  # Try relative path first (for development)
  begin
    require_relative '../lib/amoskeag_native'
  rescue LoadError
    # Fall back to standard require (for installed gem)
    require 'amoskeag_native'
  end
rescue LoadError => e
  raise LoadError, "Failed to load amoskeag_native extension. " \
                   "Make sure the gem is properly installed and compiled. " \
                   "Original error: #{e.message}"
end

# Amoskeag - A secure, functional DSL for business rules
#
# The native extension defines:
# - Amoskeag::Error < StandardError
# - Amoskeag::CompileError < Amoskeag::Error
# - Amoskeag::EvalError < Amoskeag::Error
# - Amoskeag::Program (compiled program class)
# - Amoskeag.compile(source, symbols = []) -> Program
# - Amoskeag.evaluate(program, data) -> Object
# - Amoskeag.eval_expression(source, data, symbols = []) -> Object

module Amoskeag
  class << self
    # Alias native methods for potential future wrapper use
    alias compile_native compile
    alias evaluate_native evaluate
    alias eval_expression_native eval_expression

    # Unwrap symbols from their hash representation at the top level only
    # This allows expression results like `:adult` to be returned as symbols
    # while keeping symbols nested in data structures in wrapped format
    def unwrap_symbols(value)
      case value
      when Hash
        # Check if this is a wrapped symbol at the top level
        if value.keys == ["__symbol__"]
          value["__symbol__"].to_sym
        else
          # Don't unwrap nested symbols - they're part of data structures
          value
        end
      else
        value
      end
    end

    # Recursively wrap symbols for native processing
    def wrap_symbols(value)
      case value
      when Symbol
        { "__symbol__" => value.to_s }
      when Hash
        value.transform_values { |v| wrap_symbols(v) }
      when Array
        value.map { |v| wrap_symbols(v) }
      else
        value
      end
    end

    # Validate that data is JSON-serializable
    def validate_data(data, path = "data")
      case data
      when String, Integer, Float, TrueClass, FalseClass, NilClass, Symbol
        # These are valid types (note: not all Numeric, only Integer and Float)
        data
      when Hash
        data.each do |key, value|
          unless key.is_a?(String) || key.is_a?(Symbol)
            raise ArgumentError, "#{path} key must be String or Symbol, got #{key.class}"
          end
          validate_data(value, "#{path}[#{key.inspect}]")
        end
        data
      when Array
        data.each_with_index do |item, idx|
          validate_data(item, "#{path}[#{idx}]")
        end
        data
      else
        # Invalid type - include path information
        raise ArgumentError, "#{path} contains invalid type #{data.class}"
      end
    end
  end

  # Override the compile method (no wrapping needed, just validation)
  def self.compile(source, symbols = [])
    compile_native(source, symbols)
  end

  # Override the evaluate method to wrap/unwrap symbols and validate data
  def self.evaluate(program, data)
    validate_data(data)
    result = evaluate_native(program, data)
    unwrap_symbols(result)
  end

  # Override the eval_expression method to wrap/unwrap symbols and validate data
  def self.eval_expression(source, data, symbols = [])
    validate_data(data)
    result = eval_expression_native(source, data, symbols)
    unwrap_symbols(result)
  end

  # Override Program.new to raise NoMethodError
  class Program
    def self.new(*)
      raise NoMethodError, "undefined method `new' for Amoskeag::Program"
    end
  end
end
