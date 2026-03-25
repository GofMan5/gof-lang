use crate::ast::{
    BinaryOp, EnumDecl, EnumVariant, Expr, Function, Import, Module, Param, Stmt, StructDecl,
    StructField, TypeRef, UnaryOp,
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
    format!("{indent}{}", variant.name)
}

fn format_function(function: &Function) -> String {
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
    format!("fn {head}({params}){return_annotation}:\n{body}\n",)
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
        Stmt::Return(expr, _) => format!("{indent}return {}", format_expr(expr)),
        Stmt::Bind {
            name,
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
                "{indent}{prefix}{name}{annotation} = {}",
                format_expr(value)
            )
        }
        Stmt::Assign { name, value, .. } => format!("{indent}{name} = {}", format_expr(value)),
        Stmt::If {
            condition,
            then_body,
            else_body,
            ..
        } => {
            let mut result = format!(
                "{indent}if {}:\n{}",
                format_expr(condition),
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
            format_expr(condition),
            format_block(body, indent_level + 1)
        ),
        Stmt::Match { value, arms, .. } => {
            let arms = arms
                .iter()
                .map(|arm| {
                    format!(
                        "{}{}:\n{}",
                        "    ".repeat(indent_level + 1),
                        format_expr(&arm.pattern),
                        format_block(&arm.body, indent_level + 2)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!("{indent}match {}:\n{arms}", format_expr(value))
        }
        Stmt::Select { arms, .. } => {
            let arms = arms
                .iter()
                .map(|arm| {
                    let header = match &arm.binding {
                        Some(binding) => {
                            format!("{binding} = {}", format_expr(&arm.operation))
                        }
                        None => format_expr(&arm.operation),
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
        Stmt::Expr(expr, _) => format!("{indent}{}", format_expr(expr)),
    }
}

fn format_type_ref(ty: &TypeRef) -> String {
    ty.name.clone()
}

fn format_expr(expr: &Expr) -> String {
    match expr {
        Expr::Int(value, _) => value.to_string(),
        Expr::String(value, _) => format!("{value:?}"),
        Expr::Bool(value, _) => value.to_string(),
        Expr::Ident(value, _) => value.clone(),
        Expr::List { items, .. } => format!(
            "[{}]",
            items.iter().map(format_expr).collect::<Vec<_>>().join(", ")
        ),
        Expr::Call { callee, args, .. } => format!(
            "{callee}({})",
            args.iter().map(format_expr).collect::<Vec<_>>().join(", ")
        ),
        Expr::Field { target, field, .. } => format!("{}.{}", format_expr(target), field),
        Expr::MethodCall {
            target,
            method,
            args,
            ..
        } => format!(
            "{}.{method}({})",
            format_expr(target),
            args.iter().map(format_expr).collect::<Vec<_>>().join(", ")
        ),
        Expr::Index { target, index, .. } => {
            format!("{}[{}]", format_expr(target), format_expr(index))
        }
        Expr::Go { value, .. } => format!("go {}", format_expr(value)),
        Expr::Await { value, .. } => format!("await {}", format_expr(value)),
        Expr::Unary { op, value, .. } => format!("{} {}", format_unary_op(*op), format_expr(value)),
        Expr::Binary { lhs, op, rhs, .. } => format!(
            "{} {} {}",
            format_expr(lhs),
            format_op(*op),
            format_expr(rhs)
        ),
    }
}

fn format_op(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
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

fn format_unary_op(op: UnaryOp) -> &'static str {
    match op {
        UnaryOp::Not => "not",
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
}
