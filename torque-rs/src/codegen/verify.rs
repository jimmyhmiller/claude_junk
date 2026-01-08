//! Bytecode Verification Module
//!
//! This module provides a way to verify that generated ASM code is correct
//! by simulating the JVM stack machine and checking the output.

use std::collections::HashMap;

/// Represents a JVM value on the stack or in locals
#[derive(Debug, Clone, PartialEq)]
pub enum JvmValue {
    Int(i64),
    Long(i64),
    Float(f64),
    Double(f64),
    Null,
    Object(String),  // Type name
    Array(Vec<JvmValue>),
}

/// A simple JVM stack machine simulator
#[derive(Debug)]
pub struct StackMachine {
    stack: Vec<JvmValue>,
    locals: HashMap<u32, JvmValue>,
    /// Trace of executed instructions
    trace: Vec<String>,
}

impl StackMachine {
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            locals: HashMap::new(),
            trace: Vec::new(),
        }
    }

    /// Set a local variable (for testing)
    pub fn set_local(&mut self, slot: u32, value: JvmValue) {
        self.locals.insert(slot, value);
    }

    /// Get the current stack
    pub fn stack(&self) -> &[JvmValue] {
        &self.stack
    }

    /// Get the instruction trace
    pub fn trace(&self) -> &[String] {
        &self.trace
    }

    // ========== Stack Operations ==========

    pub fn iconst(&mut self, value: i64) {
        self.trace.push(format!("ICONST {}", value));
        self.stack.push(JvmValue::Int(value));
    }

    pub fn lconst(&mut self, value: i64) {
        self.trace.push(format!("LCONST {}", value));
        self.stack.push(JvmValue::Long(value));
    }

    pub fn fconst(&mut self, value: f64) {
        self.trace.push(format!("FCONST {}", value));
        self.stack.push(JvmValue::Float(value));
    }

    pub fn dconst(&mut self, value: f64) {
        self.trace.push(format!("DCONST {}", value));
        self.stack.push(JvmValue::Double(value));
    }

    pub fn ldc(&mut self, value: JvmValue) {
        self.trace.push(format!("LDC {:?}", value));
        self.stack.push(value);
    }

    // ========== Load/Store ==========

    pub fn iload(&mut self, slot: u32) {
        self.trace.push(format!("ILOAD {}", slot));
        let value = self.locals.get(&slot).cloned().unwrap_or(JvmValue::Int(0));
        self.stack.push(value);
    }

    pub fn aload(&mut self, slot: u32) {
        self.trace.push(format!("ALOAD {}", slot));
        let value = self.locals.get(&slot).cloned().unwrap_or(JvmValue::Null);
        self.stack.push(value);
    }

    pub fn istore(&mut self, slot: u32) {
        self.trace.push(format!("ISTORE {}", slot));
        if let Some(value) = self.stack.pop() {
            self.locals.insert(slot, value);
        }
    }

    pub fn astore(&mut self, slot: u32) {
        self.trace.push(format!("ASTORE {}", slot));
        if let Some(value) = self.stack.pop() {
            self.locals.insert(slot, value);
        }
    }

    // ========== Arithmetic ==========

    pub fn iadd(&mut self) {
        self.trace.push("IADD".to_string());
        if let (Some(JvmValue::Int(b)), Some(JvmValue::Int(a))) = (self.stack.pop(), self.stack.pop()) {
            self.stack.push(JvmValue::Int(a + b));
        }
    }

    pub fn isub(&mut self) {
        self.trace.push("ISUB".to_string());
        if let (Some(JvmValue::Int(b)), Some(JvmValue::Int(a))) = (self.stack.pop(), self.stack.pop()) {
            self.stack.push(JvmValue::Int(a - b));
        }
    }

    pub fn imul(&mut self) {
        self.trace.push("IMUL".to_string());
        if let (Some(JvmValue::Int(b)), Some(JvmValue::Int(a))) = (self.stack.pop(), self.stack.pop()) {
            self.stack.push(JvmValue::Int(a * b));
        }
    }

    pub fn idiv(&mut self) {
        self.trace.push("IDIV".to_string());
        if let (Some(JvmValue::Int(b)), Some(JvmValue::Int(a))) = (self.stack.pop(), self.stack.pop()) {
            self.stack.push(JvmValue::Int(a / b));
        }
    }

    pub fn ineg(&mut self) {
        self.trace.push("INEG".to_string());
        if let Some(JvmValue::Int(a)) = self.stack.pop() {
            self.stack.push(JvmValue::Int(-a));
        }
    }

    // ========== Long Arithmetic ==========

    pub fn ladd(&mut self) {
        self.trace.push("LADD".to_string());
        if let (Some(JvmValue::Long(b)), Some(JvmValue::Long(a))) = (self.stack.pop(), self.stack.pop()) {
            self.stack.push(JvmValue::Long(a + b));
        }
    }

    // ========== Double Arithmetic ==========

    pub fn dadd(&mut self) {
        self.trace.push("DADD".to_string());
        if let (Some(JvmValue::Double(b)), Some(JvmValue::Double(a))) = (self.stack.pop(), self.stack.pop()) {
            self.stack.push(JvmValue::Double(a + b));
        }
    }

    // ========== Bitwise ==========

    pub fn iand(&mut self) {
        self.trace.push("IAND".to_string());
        if let (Some(JvmValue::Int(b)), Some(JvmValue::Int(a))) = (self.stack.pop(), self.stack.pop()) {
            self.stack.push(JvmValue::Int(a & b));
        }
    }

    pub fn ior(&mut self) {
        self.trace.push("IOR".to_string());
        if let (Some(JvmValue::Int(b)), Some(JvmValue::Int(a))) = (self.stack.pop(), self.stack.pop()) {
            self.stack.push(JvmValue::Int(a | b));
        }
    }

    pub fn ixor(&mut self) {
        self.trace.push("IXOR".to_string());
        if let (Some(JvmValue::Int(b)), Some(JvmValue::Int(a))) = (self.stack.pop(), self.stack.pop()) {
            self.stack.push(JvmValue::Int(a ^ b));
        }
    }

    pub fn ishl(&mut self) {
        self.trace.push("ISHL".to_string());
        if let (Some(JvmValue::Int(b)), Some(JvmValue::Int(a))) = (self.stack.pop(), self.stack.pop()) {
            self.stack.push(JvmValue::Int(a << (b as u32)));
        }
    }

    pub fn ishr(&mut self) {
        self.trace.push("ISHR".to_string());
        if let (Some(JvmValue::Int(b)), Some(JvmValue::Int(a))) = (self.stack.pop(), self.stack.pop()) {
            self.stack.push(JvmValue::Int(a >> (b as u32)));
        }
    }

    // ========== Stack Manipulation ==========

    pub fn dup(&mut self) {
        self.trace.push("DUP".to_string());
        if let Some(value) = self.stack.last().cloned() {
            self.stack.push(value);
        }
    }

    pub fn pop(&mut self) -> Option<JvmValue> {
        self.trace.push("POP".to_string());
        self.stack.pop()
    }

    pub fn swap(&mut self) {
        self.trace.push("SWAP".to_string());
        let len = self.stack.len();
        if len >= 2 {
            self.stack.swap(len - 1, len - 2);
        }
    }

    // ========== Comparison ==========

    pub fn if_icmpeq(&self) -> bool {
        if let (Some(JvmValue::Int(b)), Some(JvmValue::Int(a))) =
            (self.stack.last(), self.stack.get(self.stack.len().saturating_sub(2)))
        {
            return *a == *b;
        }
        false
    }

    pub fn if_icmpne(&self) -> bool {
        !self.if_icmpeq()
    }

    pub fn if_icmplt(&self) -> bool {
        if let (Some(JvmValue::Int(b)), Some(JvmValue::Int(a))) =
            (self.stack.last(), self.stack.get(self.stack.len().saturating_sub(2)))
        {
            return *a < *b;
        }
        false
    }

    pub fn if_icmpge(&self) -> bool {
        !self.if_icmplt()
    }

    /// Get the top of stack as int
    pub fn top_int(&self) -> Option<i64> {
        match self.stack.last() {
            Some(JvmValue::Int(v)) => Some(*v),
            _ => None,
        }
    }

    /// Get the top of stack
    pub fn top(&self) -> Option<&JvmValue> {
        self.stack.last()
    }

    /// Get stack size
    pub fn stack_size(&self) -> usize {
        self.stack.len()
    }
}

impl Default for StackMachine {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse ASM method calls from generated Java code and execute them
pub fn simulate_asm_method(java_source: &str) -> StackMachine {
    let mut machine = StackMachine::new();

    for line in java_source.lines() {
        let line = line.trim();

        // Parse ICONST_x instructions
        if line.contains("ICONST_0") {
            machine.iconst(0);
        } else if line.contains("ICONST_1") {
            machine.iconst(1);
        } else if line.contains("ICONST_2") {
            machine.iconst(2);
        } else if line.contains("ICONST_3") {
            machine.iconst(3);
        } else if line.contains("ICONST_4") {
            machine.iconst(4);
        } else if line.contains("ICONST_5") {
            machine.iconst(5);
        } else if line.contains("ICONST_M1") {
            machine.iconst(-1);
        }
        // Parse BIPUSH/SIPUSH
        else if line.contains("BIPUSH") || line.contains("SIPUSH") {
            if let Some(value) = extract_int_arg(line) {
                machine.iconst(value);
            }
        }
        // Parse load instructions
        else if line.contains("visitVarInsn(ILOAD") {
            if let Some(slot) = extract_slot(line) {
                machine.iload(slot);
            }
        } else if line.contains("visitVarInsn(ALOAD") {
            if let Some(slot) = extract_slot(line) {
                machine.aload(slot);
            }
        }
        // Parse store instructions
        else if line.contains("visitVarInsn(ISTORE") {
            if let Some(slot) = extract_slot(line) {
                machine.istore(slot);
            }
        } else if line.contains("visitVarInsn(ASTORE") {
            if let Some(slot) = extract_slot(line) {
                machine.astore(slot);
            }
        }
        // Parse arithmetic
        else if line.contains("visitInsn(IADD)") {
            machine.iadd();
        } else if line.contains("visitInsn(ISUB)") {
            machine.isub();
        } else if line.contains("visitInsn(IMUL)") {
            machine.imul();
        } else if line.contains("visitInsn(IDIV)") {
            machine.idiv();
        } else if line.contains("visitInsn(INEG)") {
            machine.ineg();
        } else if line.contains("visitInsn(LADD)") {
            machine.ladd();
        } else if line.contains("visitInsn(DADD)") {
            machine.dadd();
        }
        // Parse bitwise
        else if line.contains("visitInsn(IAND)") {
            machine.iand();
        } else if line.contains("visitInsn(IOR)") {
            machine.ior();
        } else if line.contains("visitInsn(IXOR)") {
            machine.ixor();
        } else if line.contains("visitInsn(ISHL)") {
            machine.ishl();
        } else if line.contains("visitInsn(ISHR)") {
            machine.ishr();
        }
        // Stack manipulation
        else if line.contains("visitInsn(DUP)") {
            machine.dup();
        }
    }

    machine
}

fn extract_int_arg(line: &str) -> Option<i64> {
    // Extract number from "BIPUSH, 42" or "SIPUSH, -100"
    let parts: Vec<&str> = line.split(',').collect();
    if parts.len() >= 2 {
        let num_part = parts[1].trim().trim_end_matches(')').trim_end_matches(';');
        num_part.parse().ok()
    } else {
        None
    }
}

fn extract_slot(line: &str) -> Option<u32> {
    // Extract slot number from "visitVarInsn(ILOAD, 0)"
    let parts: Vec<&str> = line.split(',').collect();
    if parts.len() >= 2 {
        let num_part = parts[1].trim().trim_end_matches(')').trim_end_matches(';');
        num_part.parse().ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stack_machine_add() {
        let mut machine = StackMachine::new();
        machine.iconst(5);
        machine.iconst(3);
        machine.iadd();

        assert_eq!(machine.top_int(), Some(8));
        assert_eq!(machine.stack_size(), 1);
    }

    #[test]
    fn test_stack_machine_sub() {
        let mut machine = StackMachine::new();
        machine.iconst(10);
        machine.iconst(4);
        machine.isub();

        assert_eq!(machine.top_int(), Some(6));
    }

    #[test]
    fn test_stack_machine_mul() {
        let mut machine = StackMachine::new();
        machine.iconst(7);
        machine.iconst(6);
        machine.imul();

        assert_eq!(machine.top_int(), Some(42));
    }

    #[test]
    fn test_stack_machine_locals() {
        let mut machine = StackMachine::new();
        machine.set_local(0, JvmValue::Int(100));
        machine.set_local(1, JvmValue::Int(23));

        machine.iload(0);
        machine.iload(1);
        machine.iadd();

        assert_eq!(machine.top_int(), Some(123));
    }

    #[test]
    fn test_stack_machine_bitwise() {
        let mut machine = StackMachine::new();
        machine.iconst(0b1100);
        machine.iconst(0b1010);
        machine.iand();

        assert_eq!(machine.top_int(), Some(0b1000));
    }

    #[test]
    fn test_simulate_asm_add() {
        let java_source = r#"
            mv.visitInsn(ICONST_5);
            mv.visitInsn(ICONST_3);
            mv.visitInsn(IADD);
        "#;

        let machine = simulate_asm_method(java_source);
        assert_eq!(machine.top_int(), Some(8));
    }

    #[test]
    fn test_simulate_asm_with_locals() {
        // simulate_asm_method doesn't set locals, so test manually
        let mut machine = StackMachine::new();
        machine.set_local(0, JvmValue::Int(5));
        machine.set_local(1, JvmValue::Int(7));
        machine.iload(0);
        machine.iload(1);
        machine.iadd();
        assert_eq!(machine.top_int(), Some(12));
    }
}
