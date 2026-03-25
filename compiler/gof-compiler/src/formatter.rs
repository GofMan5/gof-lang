use crate::ast::{BinaryOp, Expr, Function, Module, Stmt};

pub fn format_module(module: &Module) -> String {
    let mut output = String::new();

    for (index, function) in module.functions.iter().enumerate() {
        if index > 0 {
            output.push('\n');
        }
        output.push_str(&format_function(function));
    }

    output
}

fn format_function(function: &Function) -> String {
    let params = function.params.join(", ");
    let body = function
        .body
        .iter()
        .map(format_stmt)
        .collect::<Vec<_>>()
        .join("\n");
    format!("fn {}({params}):\n{body}\n", function.name)
}

fn format_stmt(stmt: &Stmt) -> String {
    match stmt {
        Stmt::Return(expr, _) => format!("    return {}", format_expr(expr)),
        Stmt::Bind {
            name,
            mutable,
            value,
            ..
        } => {
            let prefix = if *mutable { "mut " } else { "" };
            format!("    {prefix}{name} = {}", format_expr(value))
        }
        Stmt::Assign { name, value, .. } => format!("    {name} = {}", format_expr(value)),
        Stmt::Expr(expr, _) => format!("    {}", format_expr(expr)),
    }
}

fn format_expr(expr: &Expr) -> String {
    match expr {
        Expr::Int(value, _) => value.to_string(),
        Expr::String(value, _) => format!("{value:?}"),
        Expr::Ident(value, _) => value.clone(),
        Expr::Call { callee, args, .. } => format!(
            "{callee}({})",
            args.iter().map(format_expr).collect::<Vec<_>>().join(", ")
        ),
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
            "fn add(a, b):\n    return a + b\n\nfn main():\n    mut total = add(1,2)\n    total=total+1\n    return total\n",
        );
        let once = format_source(&source).expect("formatting should succeed");
        let twice =
            format_source(&SourceFile::new("fmt.gof", &once)).expect("reformatting should succeed");
        assert_eq!(once, twice);
    }
}
