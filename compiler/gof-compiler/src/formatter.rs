use crate::ast::{BinaryOp, Expr, Function, Import, Module, Param, Stmt, TypeRef};

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

fn format_function(function: &Function) -> String {
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
    format!(
        "fn {}({params}){return_annotation}:\n{body}\n",
        function.name
    )
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
        Expr::Index { target, index, .. } => {
            format!("{}[{}]", format_expr(target), format_expr(index))
        }
        Expr::Go { value, .. } => format!("go {}", format_expr(value)),
        Expr::Await { value, .. } => format!("await {}", format_expr(value)),
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
}
