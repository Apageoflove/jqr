use crate::schema::SchemaNode;

#[allow(clippy::too_many_arguments)]
pub fn print_plan(
    filter_str: &str,
    input_format: &str,
    output_mode: &str,
    tokens_used: usize,
    total: usize,
    truncated: bool,
    sample_size: usize,
    schema: &SchemaNode,
) {
    println!("Filter:     {filter_str}");
    println!("Input:      {input_format}");
    println!("Output:     {output_mode}");

    let estimate = if truncated {
        format!("{tokens_used} tokens (truncated to {sample_size} of {total})")
    } else {
        format!("{tokens_used} tokens ({total} result{})", if total == 1 { "" } else { "s" })
    };
    println!("Estimate:   {estimate}");

    let strategy = if truncated {
        format!("truncated to {sample_size} of {total}")
    } else {
        "full execution, no truncation needed".to_string()
    };
    println!("Strategy:   {strategy}");

    let schema_json = schema.to_compact_json();
    println!("Schema:     {schema_json}");
    println!();
    println!("Run without --explain to execute.");
}
