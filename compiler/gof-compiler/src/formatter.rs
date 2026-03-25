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
    let body = format_block(&function.body, 1);
    format!("fn {}({params}):\n{body}\n", function.name)
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
            value,
            ..
        } => {
            let prefix = if *mutable { "mut " } else { "" };
            format!("{indent}{prefix}{name} = {}", format_expr(value))
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

fn format_expr(expr: &Expr) -> String {
    match expr {
        Expr::Int(value, _) => value.to_string(),
        Expr::String(value, _) => format!("{value:?}"),
        Expr::Bool(value, _) => value.to_string(),
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
}
