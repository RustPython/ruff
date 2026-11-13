use crate::prelude::*;
use crate::verbatim::verbatim_text;
use ruff_formatter::write;
use ruff_python_ast::ExprConstant;

#[derive(Default)]
pub struct FormatExprConstant;

impl FormatNodeRule<ExprConstant> for FormatExprConstant {
    fn fmt_fields(&self, item: &ExprConstant, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [verbatim_text(item)])
    }
}
