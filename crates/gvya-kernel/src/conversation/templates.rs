//! Deterministic, allowlisted response template expression evaluator.
//!
//! No `eval`, no host calls, no ambient clock, and no ambient randomness. Template assignment is
//! restricted to author state; runtime-owned conversation state and host context are read-only.

use std::collections::BTreeMap;

use gvya_model::{ConversationState, Meaning, Value};

use super::conditions::{path_get, path_set, to_number};

const MAX_EXPANSION_PASSES: usize = 8;
pub const TEMPLATE_MAX_EXPRESSION_DEPTH: usize = 32;
pub const TEMPLATE_MAX_OUTPUT_BYTES: usize = 64 * 1024;
pub const TEMPLATE_MAX_EFFECTS: usize = 128;
pub const TEMPLATE_MAX_FUNCTION_ARGS: usize = 64;

#[derive(Clone, Debug, PartialEq)]
pub struct TemplateEnvironment {
    pub host: BTreeMap<String, Value>,
    pub system: BTreeMap<String, Value>,
    pub interaction: BTreeMap<String, Value>,
    pub meaning: Option<Meaning>,
    pub conversation: ConversationState,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TemplateEffect {
    pub path: String,
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderedTemplate {
    pub text: String,
    pub effects: Vec<TemplateEffect>,
    /// True when canonical template work/output limits were exceeded. In that case rendering
    /// fails closed to an empty text and author assignments are rolled back.
    pub limit_exceeded: bool,
}

#[derive(Clone, Debug)]
pub struct DeterministicRng {
    state: u64,
}

impl DeterministicRng {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        // SplitMix64: small, dependency-free and deterministic across targets.
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    #[must_use]
    pub fn index(&mut self, len: usize) -> Option<usize> {
        if len == 0 {
            return None;
        }
        Some((self.next_u64() % u64::try_from(len).unwrap_or(u64::MAX)) as usize)
    }

    #[must_use]
    pub fn inclusive_i64(&mut self, min: i64, max: i64) -> i64 {
        let (low, high) = if min <= max { (min, max) } else { (max, min) };
        let width = high.saturating_sub(low).saturating_add(1);
        if width <= 1 {
            return low;
        }
        low.saturating_add((self.next_u64() % u64::try_from(width).unwrap_or(u64::MAX)) as i64)
    }

    #[must_use]
    pub fn unit_f64(&mut self) -> f64 {
        let bits = self.next_u64() >> 11;
        (bits as f64) / ((1_u64 << 53) as f64)
    }
}

pub struct TemplateRenderer<'a> {
    author: &'a mut BTreeMap<String, Value>,
    env: &'a TemplateEnvironment,
    rng: DeterministicRng,
    effects: Vec<TemplateEffect>,
    exhausted: bool,
}

impl<'a> TemplateRenderer<'a> {
    pub fn new(
        author: &'a mut BTreeMap<String, Value>,
        env: &'a TemplateEnvironment,
        seed: u64,
    ) -> Self {
        Self {
            author,
            env,
            rng: DeterministicRng::new(seed),
            effects: Vec::new(),
            exhausted: false,
        }
    }

    #[must_use]
    pub fn render(mut self, input: &str) -> RenderedTemplate {
        if input.len() > TEMPLATE_MAX_OUTPUT_BYTES {
            return RenderedTemplate {
                text: String::new(),
                effects: Vec::new(),
                limit_exceeded: true,
            };
        }
        let original_author = self.author.clone();
        let mut text = input.to_string();
        if text.contains("{{") {
            // Preserve recursive template expansion while bounding passes, expression recursion,
            // effect count and every intermediate/output allocation.
            for _ in 0..MAX_EXPANSION_PASSES {
                let before = text.clone();
                text = self.render_pass(&text);
                if self.exhausted || text == before {
                    break;
                }
            }
        }
        if self.exhausted || text.len() > TEMPLATE_MAX_OUTPUT_BYTES {
            *self.author = original_author;
            return RenderedTemplate {
                text: String::new(),
                effects: Vec::new(),
                limit_exceeded: true,
            };
        }
        RenderedTemplate {
            text,
            effects: self.effects,
            limit_exceeded: false,
        }
    }

    fn render_pass(&mut self, input: &str) -> String {
        let mut out = String::with_capacity(input.len().min(TEMPLATE_MAX_OUTPUT_BYTES));
        let mut offset = 0;
        while !self.exhausted {
            let Some(relative) = input[offset..].find("{{") else {
                break;
            };
            let start = offset + relative;
            if !bounded_push(&mut out, &input[offset..start]) {
                self.exhausted = true;
                break;
            }
            let inner_start = start + 2;
            let Some(close) = find_tag_close(input, inner_start) else {
                if !bounded_push(&mut out, "{{") {
                    self.exhausted = true;
                    break;
                }
                offset = inner_start;
                continue;
            };
            let inner = &input[inner_start..close - 2];
            let evaluated = self.eval_tag(inner.trim());
            if !bounded_push(&mut out, &evaluated) {
                self.exhausted = true;
                break;
            }
            offset = close;
        }
        if !self.exhausted && !bounded_push(&mut out, &input[offset..]) {
            self.exhausted = true;
        }
        if self.exhausted { String::new() } else { out }
    }

    fn eval_tag(&mut self, inner: &str) -> String {
        if inner.is_empty() {
            return String::new();
        }
        if starts_keyword(inner, "if") {
            return self.eval_if(inner);
        }
        if let Some((left, right)) = split_assignment(inner) {
            if let Some(path) = left.trim().strip_prefix("author.") {
                if valid_state_path(path) {
                    let value = self.eval_expr(right.trim());
                    if self.effects.len() >= TEMPLATE_MAX_EFFECTS {
                        self.exhausted = true;
                        return String::new();
                    }
                    if path_set(self.author, path, value.clone()) {
                        self.effects.push(TemplateEffect {
                            path: format!("author.{path}"),
                            value,
                        });
                    }
                }
            }
            return String::new();
        }
        if let Some((path, fallback)) = split_fallback(inner) {
            if let Some(value) = self.read_path(path.trim()) {
                let formatted = format_scalar(&value);
                if !formatted.is_empty() {
                    return formatted;
                }
            }
            return trim_quotes(fallback.trim()).to_string();
        }
        if is_path(inner) {
            return self
                .read_path(inner)
                .map_or_else(String::new, |value| format_scalar(&value));
        }
        format_scalar(&self.eval_expr(inner))
    }

    fn eval_if(&mut self, input: &str) -> String {
        let body = input.trim();
        let body = body
            .strip_prefix("if")
            .or_else(|| body.strip_prefix("IF"))
            .unwrap_or(body)
            .trim();
        let (main, else_branch) = split_keyword_top_level(body, " else ");
        for clause in split_elseif_top_level(main) {
            let clause = clause.trim();
            let clause = clause.strip_prefix("if ").unwrap_or(clause).trim();
            if let Some((condition, branch)) = split_keyword_top_level_once(clause, " then ") {
                if truthy(&self.eval_expr(condition.trim())) {
                    return self.eval_branch(branch.trim());
                }
            }
        }
        else_branch.map_or_else(String::new, |branch| self.eval_branch(branch.trim()))
    }

    fn eval_branch(&mut self, branch: &str) -> String {
        let branch = branch.trim();
        if branch.len() >= 2
            && ((branch.starts_with('"') && branch.ends_with('"'))
                || (branch.starts_with('\'') && branch.ends_with('\'')))
        {
            return branch[1..branch.len() - 1].to_string();
        }
        format_scalar(&self.eval_expr(branch))
    }

    fn eval_expr(&mut self, expr: &str) -> Value {
        if self.exhausted || expr.len() > TEMPLATE_MAX_OUTPUT_BYTES {
            self.exhausted = true;
            return Value::Null;
        }
        let mut parser = ExprParser::new(expr, self);
        parser.parse_expression()
    }

    fn read_path(&self, path: &str) -> Option<Value> {
        let (root, rest) = path.split_once('.')?;
        match root {
            "author" => path_get(self.author, rest).cloned(),
            "context" => path_get(&self.env.host, rest).cloned(),
            "system" => path_get(&self.env.system, rest).cloned(),
            "interaction" => path_get(&self.env.interaction, rest).cloned(),
            "meaning" => read_meaning_path(self.env.meaning.as_ref(), rest),
            "conversation" => read_conversation_path(&self.env.conversation, rest),
            _ => None,
        }
    }

    fn call(&mut self, name: &str, args: &[Value]) -> Value {
        match name.to_ascii_lowercase().as_str() {
            "rnd" if args.len() == 2 => {
                let min = to_number(&args[0]).unwrap_or(0.0).round() as i64;
                let max = to_number(&args[1]).unwrap_or(0.0).round() as i64;
                Value::Number(self.rng.inclusive_i64(min, max) as f64)
            }
            "pick" if !args.is_empty() => self
                .rng
                .index(args.len())
                .map_or(Value::String(String::new()), |index| args[index].clone()),
            _ => Value::String(String::new()),
        }
    }
}

struct ExprParser<'r, 'a> {
    chars: Vec<char>,
    pos: usize,
    depth: usize,
    renderer: &'r mut TemplateRenderer<'a>,
}

impl<'r, 'a> ExprParser<'r, 'a> {
    fn new(text: &str, renderer: &'r mut TemplateRenderer<'a>) -> Self {
        Self {
            chars: text.chars().collect(),
            pos: 0,
            depth: 0,
            renderer,
        }
    }

    fn parse_expression(&mut self) -> Value {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Value {
        let mut left = self.parse_and();
        loop {
            self.skip_space();
            if !self.match_keyword("or") {
                break;
            }
            let right = self.parse_and();
            left = Value::Bool(truthy(&left) || truthy(&right));
        }
        left
    }

    fn parse_and(&mut self) -> Value {
        let mut left = self.parse_comparison();
        loop {
            self.skip_space();
            if !self.match_keyword("and") {
                break;
            }
            let right = self.parse_comparison();
            left = Value::Bool(truthy(&left) && truthy(&right));
        }
        left
    }

    fn parse_comparison(&mut self) -> Value {
        let left = self.parse_add();
        self.skip_space();
        for op in [">=", "<=", "==", "!=", ">", "<"] {
            if self.match_op(op) {
                let right = self.parse_add();
                return Value::Bool(compare_values(&left, op, &right));
            }
        }
        left
    }

    fn parse_add(&mut self) -> Value {
        let mut left = self.parse_mul();
        loop {
            self.skip_space();
            let Some(ch) = self.peek() else {
                break;
            };
            if ch != '+' && ch != '-' {
                break;
            }
            self.pos += 1;
            let right = self.parse_mul();
            left = if ch == '+' {
                add_values(&left, &right)
            } else {
                Value::Number(to_number(&left).unwrap_or(0.0) - to_number(&right).unwrap_or(0.0))
            };
            if matches!(&left, Value::String(value) if value.len() > TEMPLATE_MAX_OUTPUT_BYTES) {
                self.renderer.exhausted = true;
                return Value::Null;
            }
        }
        left
    }

    fn parse_mul(&mut self) -> Value {
        let mut left = self.parse_unary();
        loop {
            self.skip_space();
            let Some(ch) = self.peek() else {
                break;
            };
            if ch != '*' && ch != '/' {
                break;
            }
            self.pos += 1;
            let right = self.parse_unary();
            let l = to_number(&left).unwrap_or(0.0);
            let r = to_number(&right).unwrap_or(0.0);
            left = if ch == '*' {
                Value::Number(l * r)
            } else if r == 0.0 {
                Value::Number(0.0)
            } else {
                Value::Number(l / r)
            };
        }
        left
    }

    fn parse_unary(&mut self) -> Value {
        self.skip_space();
        if matches!(self.peek(), Some('+') | Some('-')) {
            let sign = self.peek().unwrap_or('+');
            self.pos += 1;
            if !self.enter_nested() {
                return Value::Null;
            }
            let value = to_number(&self.parse_unary()).unwrap_or(0.0);
            self.leave_nested();
            return Value::Number(if sign == '-' { -value } else { value });
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Value {
        self.skip_space();
        let Some(ch) = self.peek() else {
            return Value::Null;
        };
        if ch == '"' || ch == '\'' {
            return Value::String(self.read_string(ch));
        }
        if ch == '(' {
            self.pos += 1;
            if !self.enter_nested() {
                return Value::Null;
            }
            let value = self.parse_expression();
            self.leave_nested();
            self.skip_space();
            if self.peek() == Some(')') {
                self.pos += 1;
            }
            return value;
        }
        if ch.is_ascii_digit()
            || (ch == '-' && self.peek_next().is_some_and(|next| next.is_ascii_digit()))
        {
            return self.read_number();
        }
        let Some(mut ident) = self.read_ident() else {
            return Value::Null;
        };
        match ident.to_ascii_lowercase().as_str() {
            "true" => return Value::Bool(true),
            "false" => return Value::Bool(false),
            "null" => return Value::Null,
            _ => {}
        }
        while self.peek() == Some('.') {
            self.pos += 1;
            let Some(part) = self.read_ident() else {
                break;
            };
            ident.push('.');
            ident.push_str(&part);
        }
        self.skip_space();
        if self.peek() == Some('(') {
            self.pos += 1;
            if !self.enter_nested() {
                return Value::Null;
            }
            let args = self.parse_args();
            self.leave_nested();
            self.skip_space();
            if self.peek() == Some(')') {
                self.pos += 1;
            }
            let function_name = ident.split('.').next().unwrap_or(&ident).to_string();
            return self.renderer.call(&function_name, &args);
        }
        self.renderer.read_path(&ident).unwrap_or(Value::Null)
    }

    fn parse_args(&mut self) -> Vec<Value> {
        let mut args = Vec::new();
        self.skip_space();
        if self.peek() == Some(')') {
            return args;
        }
        loop {
            if args.len() >= TEMPLATE_MAX_FUNCTION_ARGS {
                self.renderer.exhausted = true;
                break;
            }
            args.push(self.parse_expression());
            if self.renderer.exhausted {
                break;
            }
            self.skip_space();
            if self.peek() != Some(',') {
                break;
            }
            self.pos += 1;
        }
        args
    }

    fn read_string(&mut self, quote: char) -> String {
        self.pos += 1;
        let start = self.pos;
        while self.pos < self.chars.len() {
            if self.chars[self.pos] == quote {
                let value: String = self.chars[start..self.pos].iter().collect();
                self.pos += 1;
                return value;
            }
            self.pos += 1;
        }
        self.chars[start..].iter().collect()
    }

    fn read_number(&mut self) -> Value {
        let start = self.pos;
        if self.peek() == Some('-') {
            self.pos += 1;
        }
        while self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
            self.pos += 1;
        }
        if self.peek() == Some('.') {
            self.pos += 1;
            while self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        Value::Number(text.parse::<f64>().unwrap_or(0.0))
    }

    fn read_ident(&mut self) -> Option<String> {
        self.skip_space();
        let first = self.peek()?;
        if !(first.is_ascii_alphabetic() || first == '_') {
            return None;
        }
        let start = self.pos;
        self.pos += 1;
        while self
            .peek()
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            self.pos += 1;
        }
        Some(self.chars[start..self.pos].iter().collect())
    }

    fn enter_nested(&mut self) -> bool {
        if self.depth >= TEMPLATE_MAX_EXPRESSION_DEPTH {
            self.renderer.exhausted = true;
            false
        } else {
            self.depth += 1;
            true
        }
    }

    fn leave_nested(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    fn match_keyword(&mut self, word: &str) -> bool {
        self.skip_space();
        let word_chars: Vec<char> = word.chars().collect();
        if self.pos + word_chars.len() > self.chars.len() {
            return false;
        }
        let candidate: String = self.chars[self.pos..self.pos + word_chars.len()]
            .iter()
            .collect();
        if !candidate.eq_ignore_ascii_case(word) {
            return false;
        }
        let after = self.pos + word_chars.len();
        if after < self.chars.len() && self.chars[after].is_ascii_alphanumeric() {
            return false;
        }
        self.pos = after;
        true
    }

    fn match_op(&mut self, op: &str) -> bool {
        let chars: Vec<char> = op.chars().collect();
        if self.pos + chars.len() > self.chars.len() {
            return false;
        }
        if self.chars[self.pos..self.pos + chars.len()] != chars[..] {
            return false;
        }
        self.pos += chars.len();
        true
    }

    fn skip_space(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }
    fn peek_next(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }
}

fn bounded_push(out: &mut String, value: &str) -> bool {
    let Some(next) = out.len().checked_add(value.len()) else {
        return false;
    };
    if next > TEMPLATE_MAX_OUTPUT_BYTES {
        return false;
    }
    out.push_str(value);
    true
}

#[must_use]
pub fn stable_seed(parts: &[&str], explicit: Option<u64>) -> u64 {
    if let Some(seed) = explicit {
        return seed;
    }
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for part in parts {
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

fn read_meaning_path(meaning: Option<&Meaning>, path: &str) -> Option<Value> {
    let meaning = meaning?;
    if path == "id" {
        return Some(Value::String(meaning.id.as_str().to_string()));
    }
    if let Some(slot) = path.strip_prefix("slots.") {
        return meaning
            .slots
            .iter()
            .find(|item| item.name == slot)
            .map(|item| item.value.clone());
    }
    None
}

fn read_conversation_path(state: &ConversationState, path: &str) -> Option<Value> {
    match path {
        "turnIndex" => Some(Value::Number(state.turn_index as f64)),
        "activeTopic" => Some(Value::String(
            state.active_topic.as_ref()?.id.as_str().to_string(),
        )),
        "topicTtl" => Some(Value::Number(f64::from(state.active_topic.as_ref()?.ttl))),
        "activeFollowup" => Some(Value::String(
            state.active_followup.as_ref()?.id.as_str().to_string(),
        )),
        "followupTtl" => Some(Value::Number(f64::from(
            state.active_followup.as_ref()?.ttl,
        ))),
        "lastMeaning" => Some(Value::String(
            state.last_meaning.as_ref()?.as_str().to_string(),
        )),
        "lastBehavior" => Some(Value::String(
            state.last_behavior.as_ref()?.as_str().to_string(),
        )),
        "lastTopic" => Some(Value::String(
            state.last_topic.as_ref()?.as_str().to_string(),
        )),
        "userStyle.formality" => Some(Value::String(
            match state.user_style.formality {
                gvya_model::Formality::Unknown => "unknown",
                gvya_model::Formality::Formal => "formal",
                gvya_model::Formality::Informal => "informal",
            }
            .to_string(),
        )),
        "userStyle.confidence" => Some(Value::Number(state.user_style.confidence)),
        "repairCount" => Some(Value::Number(f64::from(state.repair.consecutive))),
        "sameInputCount" => Some(Value::Number(f64::from(
            state.repeat_memory.same_input_count,
        ))),
        "sameMeaningCount" => Some(Value::Number(f64::from(
            state.repeat_memory.same_meaning_count,
        ))),
        _ => None,
    }
}

#[must_use]
pub fn format_scalar(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(value) => {
            if *value {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        Value::Number(value) => {
            if value.fract() == 0.0 {
                format!("{}", *value as i64)
            } else {
                let text = format!("{value:.10}");
                let trimmed = text.trim_end_matches('0').trim_end_matches('.');
                if trimmed == "-0" {
                    "0".to_string()
                } else {
                    trimmed.to_string()
                }
            }
        }
        Value::String(value) => value.clone(),
        Value::Array(_) | Value::Object(_) => String::new(),
    }
}

#[must_use]
pub fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => *value != 0.0,
        Value::String(value) => !value.is_empty() && value != "0",
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}

fn add_values(left: &Value, right: &Value) -> Value {
    if let (Some(a), Some(b)) = (strict_numeric(left), strict_numeric(right)) {
        Value::Number(a + b)
    } else {
        Value::String(format!("{}{}", format_scalar(left), format_scalar(right)))
    }
}

fn strict_numeric(value: &Value) -> Option<f64> {
    match value {
        Value::Number(value) => Some(*value),
        Value::String(value) if !value.is_empty() => value.parse::<f64>().ok(),
        _ => None,
    }
}

fn compare_values(left: &Value, op: &str, right: &Value) -> bool {
    if strict_numeric(left).is_some() || strict_numeric(right).is_some() {
        let l = to_number(left).unwrap_or(0.0);
        let r = to_number(right).unwrap_or(0.0);
        return compare_f64(l, op, r);
    }
    if matches!(left, Value::Bool(_)) || matches!(right, Value::Bool(_)) {
        return match op {
            "==" => truthy(left) == truthy(right),
            "!=" => truthy(left) != truthy(right),
            _ => false,
        };
    }
    let l = format_scalar(left);
    let r = format_scalar(right);
    match op {
        ">" => l > r,
        "<" => l < r,
        ">=" => l >= r,
        "<=" => l <= r,
        "==" => l == r,
        "!=" => l != r,
        _ => false,
    }
}

fn compare_f64(left: f64, op: &str, right: f64) -> bool {
    match op {
        ">" => left > right,
        "<" => left < right,
        ">=" => left >= right,
        "<=" => left <= right,
        "==" => (left - right).abs() <= f64::EPSILON,
        "!=" => (left - right).abs() > f64::EPSILON,
        _ => false,
    }
}

fn find_tag_close(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut quote: Option<u8> = None;
    let mut index = start;
    while index + 1 < bytes.len() {
        let byte = bytes[index];
        if let Some(active) = quote {
            if byte == active {
                quote = None;
            }
            index += 1;
            continue;
        }
        if byte == b'\'' || byte == b'"' {
            quote = Some(byte);
            index += 1;
            continue;
        }
        if byte == b'}' && bytes[index + 1] == b'}' {
            return Some(index + 2);
        }
        index += 1;
    }
    None
}

fn starts_keyword(text: &str, keyword: &str) -> bool {
    let text = text.trim_start();
    let Some(prefix) = text.get(..keyword.len()) else {
        return false;
    };
    prefix.eq_ignore_ascii_case(keyword)
        && text
            .get(keyword.len()..)
            .and_then(|rest| rest.chars().next())
            .is_none_or(char::is_whitespace)
}

fn split_assignment(text: &str) -> Option<(&str, &str)> {
    let bytes = text.as_bytes();
    let mut quote: Option<u8> = None;
    let mut depth = 0_i32;
    for (index, byte) in bytes.iter().copied().enumerate() {
        if let Some(active) = quote {
            if byte == active {
                quote = None;
            }
            continue;
        }
        match byte {
            b'\'' | b'"' => quote = Some(byte),
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            b'=' if depth == 0 => {
                let prev = index.checked_sub(1).and_then(|i| bytes.get(i)).copied();
                let next = bytes.get(index + 1).copied();
                if prev != Some(b'=')
                    && prev != Some(b'!')
                    && prev != Some(b'<')
                    && prev != Some(b'>')
                    && next != Some(b'=')
                {
                    return Some((&text[..index], &text[index + 1..]));
                }
            }
            _ => {}
        }
    }
    None
}

fn split_fallback(text: &str) -> Option<(&str, &str)> {
    let (left, right) = split_top_level_char(text, '|')?;
    if is_path(left.trim()) {
        Some((left, right))
    } else {
        None
    }
}

fn split_top_level_char(text: &str, needle: char) -> Option<(&str, &str)> {
    let mut quote: Option<char> = None;
    let mut depth = 0_i32;
    for (index, ch) in text.char_indices() {
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ if ch == needle && depth == 0 => {
                return Some((&text[..index], &text[index + ch.len_utf8()..]));
            }
            _ => {}
        }
    }
    None
}

fn split_keyword_top_level<'a>(text: &'a str, keyword: &str) -> (&'a str, Option<&'a str>) {
    split_keyword_top_level_once(text, keyword)
        .map_or((text, None), |(left, right)| (left, Some(right)))
}

fn split_keyword_top_level_once<'a>(text: &'a str, keyword: &str) -> Option<(&'a str, &'a str)> {
    let lower = text.to_ascii_lowercase();
    let needle = keyword.to_ascii_lowercase();
    let mut quote: Option<u8> = None;
    let mut depth = 0_i32;
    let bytes = text.as_bytes();
    let mut index = 0;
    while index + needle.len() <= text.len() {
        let byte = bytes[index];
        if let Some(active) = quote {
            if byte == active {
                quote = None;
            }
            index += 1;
            continue;
        }
        match byte {
            b'\'' | b'"' => quote = Some(byte),
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            _ => {}
        }
        if depth == 0
            && lower.as_bytes().get(index..index + needle.len()) == Some(needle.as_bytes())
        {
            return Some((&text[..index], &text[index + keyword.len()..]));
        }
        index += 1;
    }
    None
}

fn split_elseif_top_level(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut rest = text;
    loop {
        if let Some((left, right)) = split_keyword_top_level_once(rest, " elseif ") {
            parts.push(left);
            rest = right;
        } else {
            parts.push(rest);
            break;
        }
    }
    parts
}

fn trim_quotes(text: &str) -> &str {
    if text.len() >= 2
        && ((text.starts_with('"') && text.ends_with('"'))
            || (text.starts_with('\'') && text.ends_with('\'')))
    {
        &text[1..text.len() - 1]
    } else {
        text
    }
}

fn is_path(text: &str) -> bool {
    let mut parts = text.trim().split('.');
    let Some(root) = parts.next() else {
        return false;
    };
    if !matches!(
        root,
        "author" | "context" | "system" | "meaning" | "conversation"
    ) {
        return false;
    }
    parts.all(valid_identifier)
}

fn valid_state_path(path: &str) -> bool {
    !path.is_empty() && path.split('.').all(valid_identifier)
}

fn valid_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env() -> TemplateEnvironment {
        TemplateEnvironment {
            host: BTreeMap::new(),
            system: BTreeMap::new(),
            interaction: BTreeMap::new(),
            meaning: None,
            conversation: ConversationState::default(),
        }
    }

    #[test]
    fn assignment_only_writes_author_state() {
        let mut author = BTreeMap::new();
        let rendered =
            TemplateRenderer::new(&mut author, &env(), 7).render("{{ author.trust = 2 + 3 }}ok");
        assert_eq!(rendered.text, "ok");
        assert_eq!(path_get(&author, "trust"), Some(&Value::Number(5.0)));
    }

    #[test]
    fn division_by_zero_returns_zero() {
        let mut author = BTreeMap::new();
        let rendered = TemplateRenderer::new(&mut author, &env(), 7).render("{{ 9 / 0 }}");
        assert_eq!(rendered.text, "0");
    }

    #[test]
    fn deeply_nested_expression_fails_closed_without_author_mutation() {
        let mut author = BTreeMap::from([("keep".into(), Value::Number(1.0))]);
        let nested = format!(
            "{{{{ author.keep = {} }}}}",
            "(".repeat(TEMPLATE_MAX_EXPRESSION_DEPTH + 2)
                + "1"
                + &")".repeat(TEMPLATE_MAX_EXPRESSION_DEPTH + 2)
        );
        let rendered = TemplateRenderer::new(&mut author, &env(), 7).render(&nested);
        assert!(rendered.limit_exceeded);
        assert!(rendered.text.is_empty());
        assert_eq!(author.get("keep"), Some(&Value::Number(1.0)));
    }

    #[test]
    fn rendered_output_growth_is_bounded_and_rolls_back_effects() {
        let mut author = BTreeMap::new();
        let mut environment = env();
        environment.host.insert(
            "huge".into(),
            Value::String("x".repeat(TEMPLATE_MAX_OUTPUT_BYTES)),
        );
        let rendered = TemplateRenderer::new(&mut author, &environment, 7)
            .render("{{ author.changed = 1 }}a{{context.huge}}");
        assert!(rendered.limit_exceeded);
        assert!(rendered.text.is_empty());
        assert!(rendered.effects.is_empty());
        assert!(!author.contains_key("changed"));
    }

    #[test]
    fn deterministic_pick_is_stable_for_seed() {
        let mut a = BTreeMap::new();
        let mut b = BTreeMap::new();
        let left = TemplateRenderer::new(&mut a, &env(), 99).render("{{pick('a','b','c')}}");
        let right = TemplateRenderer::new(&mut b, &env(), 99).render("{{pick('a','b','c')}}");
        assert_eq!(left.text, right.text);
    }
}

/// Calculates the deliberately small arithmetic form historically exposed to conversation
/// templates: exactly two signed decimal operands and one of `+ - * /`.
#[must_use]
pub fn basic_math_result(normalized: &str) -> Option<String> {
    let compact: String = normalized
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect();
    let (left, op, right) = parse_basic_math(&compact)?;
    let result = match op {
        '+' => left + right,
        '-' => left - right,
        '*' => left * right,
        '/' if right != 0.0 => left / right,
        '/' => return None,
        _ => return None,
    };
    Some(format_math_result(result))
}

fn parse_basic_math(input: &str) -> Option<(f64, char, f64)> {
    if input.is_empty() {
        return None;
    }
    let mut operator = None;
    for (index, ch) in input.char_indices() {
        if !matches!(ch, '+' | '-' | '*' | '/') {
            continue;
        }
        if ch == '-' && index == 0 {
            continue;
        }
        let previous = input[..index].chars().last();
        if ch == '-' && previous.is_some_and(|prev| matches!(prev, '+' | '-' | '*' | '/')) {
            continue;
        }
        if operator.is_some() {
            return None;
        }
        operator = Some((index, ch));
    }
    let (index, op) = operator?;
    let left = input[..index].parse::<f64>().ok()?;
    let right = input[index + op.len_utf8()..].parse::<f64>().ok()?;
    Some((left, op, right))
}

fn format_math_result(value: f64) -> String {
    if value.fract() == 0.0 {
        return format!("{}", value as i64);
    }
    let text = format!("{value:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}
