use crate::ast::{
    BinaryOp, BindingPattern, DictEntry, EnumDecl, EnumVariant, EnumVariantField, Expr, Function,
    FunctionKind, Import, MatchPattern, Module, Param, PatternLiteral, SelectArmKind, Stmt,
    StructDecl, StructField, TypeRef, UnaryOp,
};

pub fn format_module(module: &Module) -> String {
    let mut sections = Vec::new();

    if !module.imports.is_empty() {
        sections.push(
            module
                .imports
                .iter()
                .map(format_import)
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }

    if !module.structs.is_empty() {
        sections.push(
            module
                .structs
                .iter()
                .map(format_struct)
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }

    if !module.enums.is_empty() {
        sections.push(
            module
                .enums
                .iter()
                .map(format_enum)
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }

    if !module.functions.is_empty() {
        sections.push(
            module
                .functions
                .iter()
                .map(format_function)
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }

    sections.join("\n\n")
}

fn format_import(import: &Import) -> String {
    format!("import {}", import.module)
}

fn format_struct(decl: &StructDecl) -> String {
    let fields = decl
        .fields
        .iter()
        .map(|field| format_struct_field(field, 1))
        .collect::<Vec<_>>()
        .join("\n");
    format!("struct {}:\n{fields}", decl.name)
}

fn format_struct_field(field: &StructField, indent_level: usize) -> String {
    let indent = "    ".repeat(indent_level);
    format!("{indent}{}: {}", field.name, format_type_ref(&field.ty))
}

fn format_enum(decl: &EnumDecl) -> String {
    let variants = decl
        .variants
        .iter()
        .map(|variant| format_enum_variant(variant, 1))
        .collect::<Vec<_>>()
        .join("\n");
    format!("enum {}:\n{variants}", decl.name)
}

fn format_enum_variant(variant: &EnumVariant, indent_level: usize) -> String {
    let indent = "    ".repeat(indent_level);
    if variant.fields.is_empty() {
        format!("{indent}{}", variant.name)
    } else {
        format!(
            "{indent}{}({})",
            variant.name,
            variant
                .fields
                .iter()
                .map(format_enum_variant_field)
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

fn format_enum_variant_field(field: &EnumVariantField) -> String {
    format!("{}: {}", field.name, format_type_ref(&field.ty))
}

fn format_function(function: &Function) -> String {
    let keyword = match function.kind {
        FunctionKind::Function => "fn",
        FunctionKind::Test => "test fn",
        FunctionKind::Fixture => {
            let scope = function
                .fixture_scope
                .as_ref()
                .map(|scope| scope.name.as_str())
                .unwrap_or("test");
            return format_function_with_keyword(function, &format!("fixture({scope}) fn"));
        }
    };
    format_function_with_keyword(function, keyword)
}

fn format_function_with_keyword(function: &Function, keyword: &str) -> String {
    let head = match &function.receiver_type {
        Some(receiver_type) => format!("{}.{}", format_type_ref(receiver_type), function.name),
        None => function.name.clone(),
    };
    let params = function
        .params
        .iter()
        .map(format_param)
        .collect::<Vec<_>>()
        .join(", ");
    let return_annotation = function
        .return_type
        .as_ref()
        .map(|ty| format!(" -> {}", format_type_ref(ty)))
        .unwrap_or_default();
    let body = format_block(&function.body, 1);
    format!("{keyword} {head}({params}){return_annotation}:\n{body}\n",)
}

fn format_param(param: &Param) -> String {
    match &param.ty {
        Some(ty) => format!("{}: {}", param.name, format_type_ref(ty)),
        None => param.name.clone(),
    }
}

fn format_block(stmts: &[Stmt], indent_level: usize) -> String {
    stmts
        .iter()
        .map(|stmt| format_stmt(stmt, indent_level))
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_stmt(stmt: &Stmt, indent_level: usize) -> String {
    let indent = "    ".repeat(indent_level);
    match stmt {
        // RFC 0047: statement-position expressions render through the
        // indent-aware `format_expr_at` so closure bodies (blocks nested
        // inside the expression) land at statement-relative depth instead
        // of collapsing to depth 1.
        Stmt::Return(expr, _) => {
            format!("{indent}return {}", format_expr_at(expr, indent_level))
        }
        Stmt::Bind {
            pattern,
            mutable,
            ty,
            value,
            ..
        } => {
            let prefix = if *mutable { "mut " } else { "" };
            let annotation = ty
                .as_ref()
                .map(|ty| format!(": {}", format_type_ref(ty)))
                .unwrap_or_default();
            format!(
                "{indent}{prefix}{}{annotation} = {}",
                format_binding_pattern(pattern),
                format_expr_at(value, indent_level)
            )
        }
        Stmt::Assign { name, value, .. } => {
            format!("{indent}{name} = {}", format_expr_at(value, indent_level))
        }
        Stmt::Break(_) => format!("{indent}break"),
        Stmt::Continue(_) => format!("{indent}continue"),
        Stmt::Defer(call, _) => {
            format!("{indent}defer {}", format_expr_at(call, indent_level))
        }
        Stmt::If {
            condition,
            then_body,
            else_body,
            ..
        } => {
            let mut result = format!(
                "{indent}if {}:\n{}",
                format_expr_at(condition, indent_level),
                format_block(then_body, indent_level + 1)
            );
            if !else_body.is_empty() {
                result.push_str(&format!(
                    "\n{indent}else:\n{}",
                    format_block(else_body, indent_level + 1)
                ));
            }
            result
        }
        Stmt::While {
            condition, body, ..
        } => format!(
            "{indent}while {}:\n{}",
            format_expr_at(condition, indent_level),
            format_block(body, indent_level + 1)
        ),
        Stmt::For {
            pattern,
            iterable,
            body,
            ..
        } => format!(
            "{indent}for {} in {}:\n{}",
            format_binding_pattern(pattern),
            format_expr_at(iterable, indent_level),
            format_block(body, indent_level + 1)
        ),
        Stmt::Match { value, arms, .. } => {
            let arms = arms
                .iter()
                .map(|arm| {
                    // RFC 0052: `pattern if guard:` — one space around
                    // the `if`, exactly like an `if` statement head. The
                    // guard renders at the arm-header level so nested
                    // closure blocks stay arm-relative.
                    let guard = match &arm.guard {
                        Some(guard_expr) => {
                            format!(" if {}", format_expr_at(guard_expr, indent_level + 1))
                        }
                        None => String::new(),
                    };
                    format!(
                        "{}{}{}:\n{}",
                        "    ".repeat(indent_level + 1),
                        format_match_pattern(&arm.pattern),
                        guard,
                        format_block(&arm.body, indent_level + 2)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "{indent}match {}:\n{arms}",
                format_expr_at(value, indent_level)
            )
        }
        Stmt::Select { arms, .. } => {
            let arms = arms
                .iter()
                .map(|arm| {
                    let header = match (&arm.binding, &arm.kind) {
                        (_, SelectArmKind::Default) => "default".to_string(),
                        (Some(binding), SelectArmKind::Operation { operation }) => {
                            format!("{binding} = {}", format_expr_at(operation, indent_level + 1))
                        }
                        (None, SelectArmKind::Operation { operation }) => {
                            format_expr_at(operation, indent_level + 1)
                        }
                    };
                    format!(
                        "{}{}:\n{}",
                        "    ".repeat(indent_level + 1),
                        header,
                        format_block(&arm.body, indent_level + 2)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!("{indent}select:\n{arms}")
        }
        Stmt::Expr(expr, _) => {
            format!("{indent}{}", format_expr_at(expr, indent_level))
        }
    }
}
fn format_binding_pattern(pattern: &BindingPattern) -> String {
    match pattern {
        BindingPattern::Name(name, _) => name.clone(),
        BindingPattern::Tuple(leafs, _) => format!(
            "({})",
            leafs
                .iter()
                .map(format_binding_pattern)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        // RFC 0052: `_` matches anything and binds nothing.
        BindingPattern::Wildcard(_) => "_".to_string(),
        // RFC 0052 literal leaves render exactly as the expressions that
        // construct the matched values.
        BindingPattern::Literal(literal, _) => format_pattern_literal(literal),
    }
}

fn format_pattern_literal(literal: &PatternLiteral) -> String {
    match literal {
        PatternLiteral::Int(value) => value.to_string(),
        PatternLiteral::String(value) => format!("{value:?}"),
        PatternLiteral::Bool(value) => value.to_string(),
    }
}

fn format_match_pattern(pattern: &MatchPattern) -> String {
    match pattern {
        MatchPattern::EnumVariant {
            enum_name,
            variant,
            bindings,
            ..
        } => {
            if bindings.is_empty() {
                format!("{enum_name}.{variant}")
            } else {
                format!(
                    "{enum_name}.{}({})",
                    variant,
                    bindings
                        .iter()
                        .map(format_binding_pattern)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        }
    }
}

fn format_type_ref(ty: &TypeRef) -> String {
    // RFC 0050 tuple types: the parser builds TypeRef { name: "tuple" }
    // from the parenthesized `(T1, T2)` spelling; render that spelling
    // back. The `tuple[...]` identifier form is a parser error, so a
    // round-trip must never emit it.
    if ty.name == "tuple" && !ty.args.is_empty() {
        return format!(
            "({})",
            ty.args
                .iter()
                .map(format_type_ref)
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    // RFC 0047 closure types: `TypeRef { name: "fn" }` holds the parameter
    // types followed by the return type as its last argument; render the
    // source spelling `fn(params) -> ret` back.
    if ty.name == "fn" && !ty.args.is_empty() {
        let params = &ty.args[..ty.args.len() - 1];
        let return_type = ty.args.last().expect("fn type carries a return type");
        return format!(
            "fn({}) -> {}",
            params
                .iter()
                .map(format_type_ref)
                .collect::<Vec<_>>()
                .join(", "),
            format_type_ref(return_type)
        );
    }
    if ty.args.is_empty() {
        ty.name.clone()
    } else {
        format!(
            "{}[{}]",
            ty.name,
            ty.args
                .iter()
                .map(format_type_ref)
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

pub(crate) fn format_expr(expr: &Expr) -> String {
    format_expr_at(expr, 0)
}

/// Indent-aware expression rendering: closure bodies are indented blocks
/// nested inside the expression (RFC 0047), so every recursive render
/// carries the enclosing indent level.
fn format_expr_at(expr: &Expr, indent_level: usize) -> String {
    match expr {
        Expr::Int(value, _) => value.to_string(),
        Expr::String(value, _) => format!("{value:?}"),
        Expr::Bool(value, _) => value.to_string(),
        Expr::Ident(value, _) => value.clone(),
        Expr::List { items, .. } => format!(
            "[{}]",
            items
                .iter()
                .map(|item| format_expr_at(item, indent_level))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expr::Dict { entries, .. } => format!(
            "{{{}}}",
            entries
                .iter()
                .map(|entry| format_dict_entry(entry, indent_level))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expr::Call { callee, args, .. } => format!(
            "{callee}({})",
            args.iter()
                .map(|arg| format_expr_at(arg, indent_level))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expr::Field { target, field, .. } => {
            format!("{}.{}", format_expr_at(target, indent_level), field)
        }
        Expr::MethodCall {
            target,
            method,
            args,
            ..
        } => format!(
            "{}.{method}({})",
            format_expr_at(target, indent_level),
            args.iter()
                .map(|arg| format_expr_at(arg, indent_level))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expr::Index { target, index, .. } => {
            format!(
                "{}[{}]",
                format_expr_at(target, indent_level),
                format_expr_at(index, indent_level)
            )
        }
        Expr::Go { value, .. } => format!("go {}", format_expr_at(value, indent_level)),
        Expr::Await { value, .. } => format!("await {}", format_expr_at(value, indent_level)),
        Expr::Propagate { value, .. } => format!("{}?", format_expr_at(value, indent_level)),
        Expr::Unary { op, value, .. } => match op {
            UnaryOp::Not => format!("not {}", format_expr_at(value, indent_level)),
            UnaryOp::Neg => format!("-{}", format_expr_at(value, indent_level)),
        },
        Expr::Binary { lhs, op, rhs, .. } => format!(
            "{} {} {}",
            format_expr_at(lhs, indent_level),
            format_op(*op),
            format_expr_at(rhs, indent_level)
        ),
        Expr::Tuple { items, .. } => format!(
            "({})",
            items
                .iter()
                .map(|item| format_expr_at(item, indent_level))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expr::TupleIndex { target, index, .. } => {
            format!("{}.{index}", format_expr_at(target, indent_level))
        }
        Expr::Closure {
            params,
            return_type,
            body,
            ..
        } => {
            let params = params
                .iter()
                .map(|param| match &param.ty {
                    Some(ty) => format!("{}: {}", param.name, format_type_ref(ty)),
                    None => param.name.clone(),
                })
                .collect::<Vec<_>>()
                .join(", ");
            let return_annotation = return_type
                .as_ref()
                .map(|ty| format!(" -> {}", format_type_ref(ty)))
                .unwrap_or_default();
            format!(
                "fn ({params}){return_annotation}:\n{}",
                format_block(body, indent_level + 1)
            )
        }
    }
}

fn format_dict_entry(entry: &DictEntry, indent_level: usize) -> String {
    format!(
        "{}: {}",
        format_expr_at(&entry.key, indent_level),
        format_expr_at(&entry.value, indent_level)
    )
}

fn format_op(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Mod => "%",
        BinaryOp::And => "and",
        BinaryOp::Or => "or",
        BinaryOp::Eq => "==",
        BinaryOp::Ne => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::Le => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::Ge => ">=",
    }
}

#[cfg(test)]
mod tests {
    use crate::format_source;
    use crate::source::SourceFile;

    #[test]
    fn formatter_is_deterministic() {
        let source = SourceFile::new(
            "fmt.gof",
            "fn sum_to(limit):\n    mut total=0\n    mut current=1\n    while current<=limit:\n        total=total+current\n        current=current+1\n    if total>10:\n        return true\n    else:\n        return false\n",
        );
        let once = format_source(&source).expect("formatting should succeed");
        let twice =
            format_source(&SourceFile::new("fmt.gof", &once)).expect("reformatting should succeed");
        assert_eq!(once, twice);
    }

    #[test]
    fn formatter_supports_go_and_await() {
        let source = SourceFile::new(
            "fmt.gof",
            "import worker\n\nfn work(x:int)->int:\n    return x*x\nfn main()->int:\n    values=[1,2,3]\n    task:task=go work(values[1])\n    return await task + len(values)\n",
        );
        let formatted = format_source(&source).expect("formatting should succeed");
        assert_eq!(
            formatted,
            "import worker\n\nfn work(x: int) -> int:\n    return x * x\n\nfn main() -> int:\n    values = [1, 2, 3]\n    task: task = go work(values[1])\n    return await task + len(values)\n"
        );
    }

    #[test]
    fn formatter_supports_structs_and_fields() {
        let source = SourceFile::new(
            "fmt.gof",
            "struct Point:\n    x:int\n    y:int\n\nfn main()->int:\n    point:Point=Point(3,4)\n    return point.x+point.y\n",
        );
        let formatted = format_source(&source).expect("formatting should succeed");
        assert_eq!(
            formatted,
            "struct Point:\n    x: int\n    y: int\n\nfn main() -> int:\n    point: Point = Point(3, 4)\n    return point.x + point.y\n"
        );
    }

    #[test]
    fn formatter_supports_logical_operators() {
        let source = SourceFile::new(
            "fmt.gof",
            "fn main()->bool:\n    return not false and true or false\n",
        );
        let formatted = format_source(&source).expect("formatting should succeed");
        assert_eq!(
            formatted,
            "fn main() -> bool:\n    return not false and true or false\n"
        );
    }

    #[test]
    fn formatter_supports_for_in_loops() {
        let source = SourceFile::new(
            "fmt.gof",
            "fn main()->int:\n    total=0\n    for value in [1,2,3]:\n        total=total+value\n    return total\n",
        );
        let formatted = format_source(&source).expect("formatting should succeed");
        assert_eq!(
            formatted,
            "fn main() -> int:\n    total = 0\n    for value in [1, 2, 3]:\n        total = total + value\n    return total\n"
        );
    }

    #[test]
    fn formatter_supports_break_and_continue() {
        let source = SourceFile::new(
            "fmt.gof",
            "fn main()->int:\n    mut total=0\n    for value in [1,2,3]:\n        if value==2:\n            continue\n        total=total+value\n        if total>3:\n            break\n    return total\n",
        );
        let formatted = format_source(&source).expect("formatting should succeed");
        assert_eq!(
            formatted,
            "fn main() -> int:\n    mut total = 0\n    for value in [1, 2, 3]:\n        if value == 2:\n            continue\n        total = total + value\n        if total > 3:\n            break\n    return total\n"
        );
    }

    #[test]
    fn formatter_supports_dict_literals() {
        let source = SourceFile::new(
            "fmt.gof",
            "fn main()->int:\n    values:dict={\"ok\":2,\"warn\":3}\n    return values[\"ok\"]+len(values)\n",
        );
        let formatted = format_source(&source).expect("formatting should succeed");
        assert_eq!(
            formatted,
            "fn main() -> int:\n    values: dict = {\"ok\": 2, \"warn\": 3}\n    return values[\"ok\"] + len(values)\n"
        );
    }

    #[test]
    fn formatter_supports_enums_and_variant_references() {
        let source = SourceFile::new(
            "fmt.gof",
            "enum Status:\n    Ready\n    Busy\n\nfn main()->bool:\n    return Status.Ready==Status.Busy\n",
        );
        let formatted = format_source(&source).expect("formatting should succeed");
        assert_eq!(
            formatted,
            "enum Status:\n    Ready\n    Busy\n\nfn main() -> bool:\n    return Status.Ready == Status.Busy\n"
        );
    }

    #[test]
    fn formatter_supports_match_arms() {
        let source = SourceFile::new(
            "fmt.gof",
            "enum Status:\n    Ready\n    Busy\n\nfn main()->int:\n    current:Status=Status.Ready\n    match current:\n        Status.Ready:\n            return 1\n        Status.Busy:\n            return 2\n",
        );
        let formatted = format_source(&source).expect("formatting should succeed");
        assert_eq!(
            formatted,
            "enum Status:\n    Ready\n    Busy\n\nfn main() -> int:\n    current: Status = Status.Ready\n    match current:\n        Status.Ready:\n            return 1\n        Status.Busy:\n            return 2\n"
        );
    }

    #[test]
    fn formatter_supports_payload_enums_and_patterns() {
        let source = SourceFile::new(
            "fmt.gof",
            "enum JobState:\n    Ready\n    Running(pid:int)\n    Failed(message:string)\n\nfn main()->int:\n    state=JobState.Running(41)\n    match state:\n        JobState.Ready:\n            return 0\n        JobState.Running(pid):\n            return pid\n        JobState.Failed(message):\n            return len(message)\n",
        );
        let formatted = format_source(&source).expect("formatting should succeed");
        assert_eq!(
            formatted,
            "enum JobState:\n    Ready\n    Running(pid: int)\n    Failed(message: string)\n\nfn main() -> int:\n    state = JobState.Running(41)\n    match state:\n        JobState.Ready:\n            return 0\n        JobState.Running(pid):\n            return pid\n        JobState.Failed(message):\n            return len(message)\n"
        );
    }

    #[test]
    fn formatter_supports_receiver_methods() {
        let source = SourceFile::new(
            "fmt.gof",
            "struct Point:\n    x:int\n    y:int\n\nfn Point.total(self:Point, extra:int)->int:\n    return self.x+self.y+extra\n\nfn main()->int:\n    point:Point=Point(3,4)\n    return point.total(5)\n",
        );
        let formatted = format_source(&source).expect("formatting should succeed");
        assert_eq!(
            formatted,
            "struct Point:\n    x: int\n    y: int\n\nfn Point.total(self: Point, extra: int) -> int:\n    return self.x + self.y + extra\n\nfn main() -> int:\n    point: Point = Point(3, 4)\n    return point.total(5)\n"
        );
    }

    #[test]
    fn formatter_supports_select_arms() {
        let source = SourceFile::new(
            "fmt.gof",
            "fn main()->int:\n    left:channel=channel()\n    right:channel=channel()\n    select:\n        value=recv(left):\n            return value\n        recv(right):\n            return 2\n",
        );
        let formatted = format_source(&source).expect("formatting should succeed");
        assert_eq!(
            formatted,
            "fn main() -> int:\n    left: channel = channel()\n    right: channel = channel()\n    select:\n        value = recv(left):\n            return value\n        recv(right):\n            return 2\n"
        );
    }

    #[test]
    fn formatter_supports_select_default_arm() {
        let source = SourceFile::new(
            "fmt.gof",
            "fn main()->int:\n    select:\n        default:\n            return 1\n",
        );
        let formatted = format_source(&source).expect("formatting should succeed");
        assert_eq!(
            formatted,
            "fn main() -> int:\n    select:\n        default:\n            return 1\n"
        );
    }

    #[test]
    fn formatter_supports_select_send_arms() {
        let source = SourceFile::new(
            "fmt.gof",
            "fn main()->int:\n    ch:channel=channel(1)\n    select:\n        sent=send(ch,7):\n            return 1\n        send(ch,8):\n            return 2\n",
        );
        let formatted = format_source(&source).expect("formatting should succeed");
        assert_eq!(
            formatted,
            "fn main() -> int:\n    ch: channel = channel(1)\n    select:\n        sent = send(ch, 7):\n            return 1\n        send(ch, 8):\n            return 2\n"
        );
    }

    #[test]
    fn formatter_supports_result_annotations_and_propagation() {
        let source = SourceFile::new(
            "fmt.gof",
            "fn parse_port()->Result[int,string]:\n    return Result.Ok(41)\n\nfn main()->Result[int,string]:\n    port=parse_port()?\n    return Result.Ok(port+1)\n",
        );
        let formatted = format_source(&source).expect("formatting should succeed");
        assert_eq!(
            formatted,
            "fn parse_port() -> Result[int, string]:\n    return Result.Ok(41)\n\nfn main() -> Result[int, string]:\n    port = parse_port()?\n    return Result.Ok(port + 1)\n"
        );
    }

    #[test]
    fn formatter_supports_test_functions() {
        let source = SourceFile::new(
            "math_test.gof",
            "import testing\n\ntest fn truthy_case(t:TestContext):\n    t.true(true,\"expected truth\")\n",
        );
        let formatted = format_source(&source).expect("formatting should succeed");
        assert_eq!(
            formatted,
            "import testing\n\ntest fn truthy_case(t: TestContext):\n    t.true(true, \"expected truth\")\n"
        );
    }

    #[test]
    fn formatter_supports_fixture_functions() {
        let source = SourceFile::new(
            "math_test.gof",
            "fixture(module) fn shared_total()->int:\n    return 41\n",
        );
        let formatted = format_source(&source).expect("formatting should succeed");
        assert_eq!(
            formatted,
            "fixture(module) fn shared_total() -> int:\n    return 41\n"
        );
    }

    #[test]
    fn formatter_supports_guards_and_full_pattern_leaves() {
        let source = SourceFile::new(
            "fmt.gof",
            "enum Lamp:\n    On(level: int)\n    Off\n\nfn main() -> int:\n    lamp: Lamp = Lamp.On(4)\n    match lamp:\n        Lamp.On(404) if 404 > 400:\n            return 1\n        Lamp.On(-1):\n            return 2\n        Lamp.On(_):\n            return 3\n        Lamp.Off:\n            return 4\n    (_, y) = (1, 2)\n    for (i, _) in [(1, 2)]:\n        print(i)\n    _ = y\n    return y\n",
        );
        let formatted = format_source(&source).expect("formatting should succeed");
        let expected = "enum Lamp:\n    On(level: int)\n    Off\n\nfn main() -> int:\n    lamp: Lamp = Lamp.On(4)\n    match lamp:\n        Lamp.On(404) if 404 > 400:\n            return 1\n        Lamp.On(-1):\n            return 2\n        Lamp.On(_):\n            return 3\n        Lamp.Off:\n            return 4\n    (_, y) = (1, 2)\n    for (i, _) in [(1, 2)]:\n        print(i)\n    _ = y\n    return y\n";
        assert_eq!(formatted, expected);
        // Round-trip: formatting the formatted output is a fixed point.
        let reparsed = SourceFile::new("fmt.gof", formatted);
        assert_eq!(
            format_source(&reparsed).expect("reformatting should succeed"),
            expected
        );
    }

    #[test]
    fn formatter_renders_string_and_bool_pattern_literals() {
        let source = SourceFile::new(
            "fmt.gof",
            "enum Code:\n    Named(name: string)\n    Flag(active: bool)\n\nfn main() -> int:\n    code: Code = Code.Named(\"x\")\n    match code:\n        Code.Named(\"alpha\"):\n            return 1\n        Code.Flag(true):\n            return 2\n        Code.Named(name):\n            return len(name)\n        Code.Flag(active):\n            return 3\n",
        );
        let formatted = format_source(&source).expect("formatting should succeed");
        assert_eq!(
            formatted,
            "enum Code:\n    Named(name: string)\n    Flag(active: bool)\n\nfn main() -> int:\n    code: Code = Code.Named(\"x\")\n    match code:\n        Code.Named(\"alpha\"):\n            return 1\n        Code.Flag(true):\n            return 2\n        Code.Named(name):\n            return len(name)\n        Code.Flag(active):\n            return 3\n"
        );
    }
}
