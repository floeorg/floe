//! TypeScript type parsing: TsType enum and type string parser.

/// A field in a TypeScript object type, tracking optionality.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectField {
    pub name: String,
    pub ty: TsType,
    pub optional: bool,
}

/// A parameter in a TypeScript function type, tracking optionality.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionParam {
    pub ty: TsType,
    pub optional: bool,
}

impl TsType {
    /// Returns true if this type contains null or undefined (directly or in a union).
    pub fn is_nullable(&self) -> bool {
        match self {
            TsType::Null | TsType::Undefined => true,
            TsType::Union(parts) => parts
                .iter()
                .any(|p| matches!(p, TsType::Null | TsType::Undefined)),
            _ => false,
        }
    }
}

/// A raw TypeScript type as parsed from .d.ts files, before boundary wrapping.
#[derive(Debug, Clone, PartialEq)]
pub enum TsType {
    /// Primitive: string, number, boolean, void, never
    Primitive(String),
    /// `null`
    Null,
    /// `undefined`
    Undefined,
    /// `any`
    Any,
    /// `unknown`
    Unknown,
    /// Named type reference: `Element`, `HTMLDivElement`
    Named(String),
    /// Generic type: `Promise<T>`, `Array<T>`
    Generic { name: String, args: Vec<TsType> },
    /// Union: `T | U | V`
    Union(Vec<TsType>),
    /// Function: `(params) => ReturnType`
    Function {
        params: Vec<FunctionParam>,
        return_type: Box<TsType>,
    },
    /// Array shorthand: `T[]`
    Array(Box<TsType>),
    /// Object type / record
    Object(Vec<ObjectField>),
    /// Tuple: `[T, U]`
    Tuple(Vec<TsType>),
    /// String literal type: `"up"`, `"down"` — preserved so unions of
    /// literals keep their discriminators at the Floe boundary.
    StringLiteral(String),
    /// Numeric literal type: `0`, `404`.
    NumberLiteral(f64),
    /// Boolean literal type: `true`, `false`.
    BooleanLiteral(bool),
    /// `this` return type — resolved to the enclosing interface/class name
    /// during conversion so fluent builders keep a usable type.
    This,
}

/// Convert a TsType to a human-readable string for display.
pub fn ts_type_to_string(ty: &TsType) -> String {
    match ty {
        TsType::Primitive(s) => s.clone(),
        TsType::Null => "null".to_string(),
        TsType::Undefined => "undefined".to_string(),
        TsType::Any => "any".to_string(),
        TsType::Unknown => "unknown".to_string(),
        TsType::Named(n) => n.clone(),
        TsType::Generic { name, args } => {
            let args_str: Vec<String> = args.iter().map(ts_type_to_string).collect();
            format!("{}<{}>", name, args_str.join(", "))
        }
        TsType::Union(parts) => {
            let parts_str: Vec<String> = parts.iter().map(ts_type_to_string).collect();
            parts_str.join(" | ")
        }
        TsType::Function {
            params,
            return_type,
        } => {
            let params_str: Vec<String> = params.iter().map(|p| ts_type_to_string(&p.ty)).collect();
            format!(
                "({}) -> {}",
                params_str.join(", "),
                ts_type_to_string(return_type)
            )
        }
        TsType::Array(inner) => format!("Array<{}>", ts_type_to_string(inner)),
        TsType::Object(fields) => {
            let fs: Vec<String> = fields
                .iter()
                .map(|f| {
                    let opt = if f.optional { "?" } else { "" };
                    format!("{}{}: {}", f.name, opt, ts_type_to_string(&f.ty))
                })
                .collect();
            format!("{{ {} }}", fs.join(", "))
        }
        TsType::Tuple(parts) => {
            let ps: Vec<String> = parts.iter().map(ts_type_to_string).collect();
            format!("[{}]", ps.join(", "))
        }
        TsType::StringLiteral(s) => format!("\"{s}\""),
        TsType::NumberLiteral(n) => n.to_string(),
        TsType::BooleanLiteral(b) => b.to_string(),
        TsType::This => "this".to_string(),
    }
}

/// Parses a TypeScript type string into a TsType.
///
/// Legacy: only used by test helper functions now that the main parser uses oxc.
#[cfg(test)]
pub(super) fn parse_type_str(s: &str) -> TsType {
    let s = s.trim();

    if s.is_empty() {
        return TsType::Primitive("void".to_string());
    }

    // Union types: T | U | V (split at top-level |)
    let union_parts = split_at_top_level(s, '|');
    if union_parts.len() > 1 {
        let parts: Vec<TsType> = union_parts
            .iter()
            .map(|part| parse_type_str(part.trim()))
            .collect();
        return TsType::Union(parts);
    }

    // Array shorthand: T[]
    if let Some(inner) = s.strip_suffix("[]") {
        return TsType::Array(Box::new(parse_type_str(inner)));
    }

    // Tuple: [T, U]
    if s.starts_with('[') && s.ends_with(']') {
        let inner = &s[1..s.len() - 1];
        let parts = split_at_top_level(inner, ',');
        return TsType::Tuple(parts.iter().map(|p| parse_type_str(p.trim())).collect());
    }

    // Function type: (params) => ReturnType
    if s.starts_with('(')
        && let Some(close) = find_matching_paren(s)
    {
        let params_str = &s[1..close];
        let after = s[close + 1..].trim();
        if let Some(ret_str) = after.strip_prefix("->") {
            let params = parse_param_types(params_str);
            let return_type = parse_type_str(ret_str.trim());
            return TsType::Function {
                params: params
                    .into_iter()
                    .map(|(ty, optional)| FunctionParam { ty, optional })
                    .collect(),
                return_type: Box::new(return_type),
            };
        }
    }

    // Generic: Name<T, U>
    if let Some(angle) = s.find('<')
        && s.ends_with('>')
    {
        let name = s[..angle].trim().to_string();
        let args_str = &s[angle + 1..s.len() - 1];
        let args = split_at_top_level(args_str, ',');
        let args: Vec<TsType> = args.iter().map(|a| parse_type_str(a.trim())).collect();

        // Normalize Array<T> to array
        if name == "Array" && args.len() == 1 {
            return TsType::Array(Box::new(args.into_iter().next().unwrap()));
        }

        return TsType::Generic { name, args };
    }

    // Object literal: { ... }
    if s.starts_with('{') && s.ends_with('}') {
        let inner = &s[1..s.len() - 1].trim();
        if inner.is_empty() {
            return TsType::Object(Vec::new());
        }
        let parts = split_at_top_level(inner, ';');
        let fields: Vec<ObjectField> = parts
            .iter()
            .filter_map(|part| {
                let part = part.trim();
                if part.is_empty() {
                    return None;
                }
                let colon = part.find(':')?;
                let raw_name = part[..colon].trim().trim_start_matches("readonly ");
                let optional = raw_name.ends_with('?');
                let name = raw_name.trim_end_matches('?').to_string();
                let ty = parse_type_str(part[colon + 1..].trim());
                Some(ObjectField { name, ty, optional })
            })
            .collect();
        return TsType::Object(fields);
    }

    // String literal: "foo" or 'foo'
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        let inner = &s[1..s.len() - 1];
        return TsType::StringLiteral(inner.to_string());
    }

    // Boolean literal
    if s == "true" {
        return TsType::BooleanLiteral(true);
    }
    if s == "false" {
        return TsType::BooleanLiteral(false);
    }

    // Numeric literal
    if let Ok(n) = s.parse::<f64>() {
        return TsType::NumberLiteral(n);
    }

    // Primitives and special types
    match s {
        "string" => TsType::Primitive("string".to_string()),
        "number" => TsType::Primitive("number".to_string()),
        "boolean" => TsType::Primitive("boolean".to_string()),
        "void" => TsType::Primitive("void".to_string()),
        "never" => TsType::Primitive("never".to_string()),
        "bigint" => TsType::Primitive("bigint".to_string()),
        "symbol" => TsType::Primitive("symbol".to_string()),
        "null" => TsType::Null,
        "undefined" => TsType::Undefined,
        "any" => TsType::Any,
        "unknown" => TsType::Unknown,
        "this" => TsType::This,
        _ => TsType::Named(s.to_string()),
    }
}

/// Parse parameter types from a param string like "x: string, y?: number".
/// Returns (type, optional) pairs.
#[cfg(test)]
pub(super) fn parse_param_types(params_str: &str) -> Vec<(TsType, bool)> {
    if params_str.trim().is_empty() {
        return Vec::new();
    }
    let parts = split_at_top_level(params_str, ',');
    parts
        .iter()
        .filter_map(|part| {
            let part = part.trim();
            if part.is_empty() {
                return None;
            }
            // "name: Type" or "name?: Type" or "...name: Type"
            if let Some(colon) = part.find(':') {
                let before_colon = part[..colon].trim();
                let optional = before_colon.ends_with('?');
                Some((parse_type_str(part[colon + 1..].trim()), optional))
            } else {
                // Bare type (rare in .d.ts but handle it)
                Some((parse_type_str(part), false))
            }
        })
        .collect()
}

/// Split a string at top-level occurrences of a delimiter (not inside <>, (), [], {}).
#[cfg(test)]
pub(super) fn split_at_top_level(s: &str, delim: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32; // tracks <, (, [, {

    for ch in s.chars() {
        match ch {
            '<' | '(' | '[' | '{' => {
                depth += 1;
                current.push(ch);
            }
            '>' | ')' | ']' | '}' => {
                depth -= 1;
                current.push(ch);
            }
            c if c == delim && depth == 0 => {
                parts.push(current.clone());
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() || !parts.is_empty() {
        parts.push(current);
    }

    parts
}

/// Find the matching close parenthesis in a string starting with '('.
#[cfg(test)]
pub(super) fn find_matching_paren(s: &str) -> Option<usize> {
    let mut depth = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}
