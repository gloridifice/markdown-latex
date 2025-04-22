use clap::Parser;
use pulldown_cmark::{Event, HeadingLevel, Options, Parser as MdParser, Tag};
use regex::Regex;
use std::collections::HashMap;
use std::fs::{File, read_to_string};
use std::io::Write;

/// 命令行参数
#[derive(Parser, Debug)]
#[command(about = "将 Markdown 内容转换为 LaTeX")]
struct Args {
    /// 输入的 Markdown 文件
    #[arg(short, long)]
    input: String,

    /// 输出的 LaTeX 文件
    #[arg(short, long)]
    output: String,
}

fn main() -> std::io::Result<()> {
    let args = Args::parse();
    let md_content = read_to_string(&args.input)?;

    let converted = convert_markdown_to_latex(&md_content);

    let mut file = File::create(&args.output)?;
    writeln!(file, "{}", converted)?;

    Ok(())
}

fn convert_markdown_to_latex(markdown: &str) -> String {
    let preprocessed = replace_block_equations(markdown);

    let mut output = String::new();
    let parser = MdParser::new_ext(&preprocessed, Options::all());

    let replacements = build_replacement_table();
    let mut _inside_paragraph = false;

    for event in parser {
        match event {
            Event::Start(Tag::Heading(level, _, _)) => {
                output.push_str("\\");
                output.push_str(match level {
                    HeadingLevel::H1 => "chapter{",
                    HeadingLevel::H2 => "section{",
                    HeadingLevel::H3 => "subsection{",
                    HeadingLevel::H4 => "subsubsection{",
                    HeadingLevel::H5 => "paragraph{",
                    _ => "textbf{",
                });
            }
            Event::End(Tag::Heading(_, _, _)) => output.push_str("}\n"),
            Event::Start(Tag::Paragraph) => {
                _inside_paragraph = true;
                // 可选：插入 \par 或开头标记
                // output.push_str("\\par\n");
            }
            Event::End(Tag::Paragraph) => {
                _inside_paragraph = false;
                output.push_str("\n\n"); // 两个换行表示段落结束
            }
            Event::Text(text) => {
                output.push_str(&apply_text_replacements(&text, &replacements));
            }
            Event::Start(Tag::Emphasis) => output.push_str("\\textit{"),
            Event::End(Tag::Emphasis) => output.push('}'),
            Event::Start(Tag::Strong) => output.push_str("\\textbf{"),
            Event::End(Tag::Strong) => output.push('}'),
            Event::Start(Tag::Link(_href, url, _)) => {
                output.push_str(&format!("\\href{{{}}}{{", url));
            }
            Event::End(Tag::Link(_, _, _)) => output.push('}'),
            Event::Code(code) => {
                // 行内代码可映射为 \texttt{}
                output.push_str(&format!("\\texttt{{{}}}", code));
            }
            Event::Rule => output.push_str("\\hrulefill\n"),
            Event::SoftBreak => {
                // 对应 Markdown 中的普通换行（段内换行）
                output.push_str("\n"); // 或者 "\\\\\n" 如果你希望在 LaTeX 中也表现为换行
            }
            Event::HardBreak => {
                // 对应 Markdown 中以空格+换行表示的强制换行（行尾 \ 或空两格）
                output.push_str("\\\\\n");
            }
            _ => {}
        }
    }

    output
}

fn replace_block_equations(input: &str) -> String {
    let mut output = String::new();
    let mut lines = input.lines().peekable();

    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.starts_with("$$$") {
            // 开始公式块
            let label = trimmed.trim_start_matches("$$$").trim();
            let mut content = String::new();

            // 收集中间内容
            while let Some(next_line) = lines.peek() {
                if next_line.trim() == "$$$" {
                    lines.next(); // consume closing
                    break;
                } else {
                    content.push_str(lines.next().unwrap());
                    content.push('\n');
                }
            }

            output.push_str("\\begin{equation}\n");
            output.push_str(&content);
            output.push_str(&format!("\\label{{eq:{}}}\n", label));
            output.push_str("\\end{equation}\n\n");
        } else {
            output.push_str(line);
            output.push('\n');
        }
    }

    output
}

/// 映射表构建（来源于 PDF 表格）
fn build_replacement_table() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        // 文本模式替换
        ("**", "\\textbf{"),
        ("*", "\\textit{"),
        ("[`", "\\cite{"),
        ("`]", "}"),
        // 语法结构替换（部分会被前面代码覆盖）
        ("$", "$"), // 保持公式不变
                    // 块级公式、图片等可另外处理
    ])
}

/// 对纯文本做替换
fn apply_text_replacements(text: &str, table: &HashMap<&str, &str>) -> String {
    let mut replaced = text.to_string();

    // 简单替换逻辑（这里可以加入 regex 做结构化处理）
    for (md, latex) in table {
        replaced = replaced.replace(md, latex);
    }

    // 对类似 [text](url) 的 Markdown 链接用正则替换为 \href{url}{text}
    let re = Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap();
    replaced = re.replace_all(&replaced, "\\href{$2}{$1}").to_string();

    replaced
}
