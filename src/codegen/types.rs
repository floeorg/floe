use super::*;

impl Codegen {
    // ── Type Expressions ─────────────────────────────────────────

    pub(super) fn emit_type_expr(&mut self, type_expr: &TypeExpr) {
        match &type_expr.kind {
            TypeExprKind::Named {
                name, type_args, ..
            } => {
                // Option<T> becomes T | null | undefined
                // Accepts both null (from serde/JSON) and undefined (from Floe's None)
                if name == type_layout::TYPE_OPTION && type_args.len() == 1 {
                    self.emit_type_expr(&type_args[0]);
                    self.push(" | null | undefined");
                    return;
                }
                // Settable<T> becomes T | null | undefined
                if name == type_layout::TYPE_SETTABLE && type_args.len() == 1 {
                    self.emit_type_expr(&type_args[0]);
                    self.push(" | null | undefined");
                    return;
                }
                // Result<T, E> becomes { ok: true; value: T } | { ok: false; error: E }
                if name == type_layout::TYPE_RESULT && type_args.len() == 2 {
                    self.push(&format!("{{ {OK_FIELD}: true; {VALUE_FIELD}: "));
                    self.emit_type_expr(&type_args[0]);
                    self.push(&format!(" }} | {{ {OK_FIELD}: false; {ERROR_FIELD}: "));
                    self.emit_type_expr(&type_args[1]);
                    self.push(" }");
                    return;
                }

                // Unit type () becomes void in TypeScript
                if name == type_layout::TYPE_UNIT {
                    self.push("void");
                    return;
                }

                self.push(name);
                if !type_args.is_empty() {
                    self.push("<");
                    for (i, arg) in type_args.iter().enumerate() {
                        if i > 0 {
                            self.push(", ");
                        }
                        self.emit_type_expr(arg);
                    }
                    self.push(">");
                }
            }
            TypeExprKind::Record(fields) => {
                self.emit_record_type(fields);
            }
            TypeExprKind::Function {
                params,
                return_type,
            } => {
                self.push("(");
                for (i, param) in params.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.push(&format!("_p{i}: "));
                    self.emit_type_expr(param);
                }
                self.push(") => ");
                self.emit_type_expr(return_type);
            }
            TypeExprKind::Array(inner) => {
                self.emit_type_expr(inner);
                self.push("[]");
            }
            TypeExprKind::Tuple(types) => {
                self.push("readonly [");
                for (i, t) in types.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.emit_type_expr(t);
                }
                self.push("]");
            }
            TypeExprKind::TypeOf(name) => {
                self.push(&format!("typeof {name}"));
            }
            TypeExprKind::Intersection(types) => {
                for (i, ty) in types.iter().enumerate() {
                    if i > 0 {
                        self.push(" & ");
                    }
                    self.emit_type_expr(ty);
                }
            }
            TypeExprKind::StringLiteral(value) => {
                self.push(&format!("\"{value}\""));
            }
        }
    }
}
