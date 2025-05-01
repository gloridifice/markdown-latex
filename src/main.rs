use clap::Parser;
use pulldown_cmark::{
    CodeBlockKind, CowStr, Event, HeadingLevel, Options, Parser as MdParser, Tag,
};
use regex::Regex;
use std::{borrow::Cow, collections::HashMap, path::Path, sync::LazyLock};

static PRE_REPLACEMENT_TABLE: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    HashMap::from([
        ("[`", "\\cite{"),
        ("`]", "}"),
        ("[*", "\\ref{"),
        ("*]", "}"),
    ])
});

static IN_TEXT_REPLACEMENT_TABLE: LazyLock<HashMap<&'static str, &'static str>> =
    LazyLock::new(|| {
        HashMap::from([
            ("&", "\\&"),
            ("%", "\\%"),
            ("_", "\\_"),
            ("#", "\\#"),
            ("{", "\\{"),
            ("}", "\\}"),
        ])
    });

/// 将 Markdown 文件中的内容转换为 LaTeX（支持语法映射与公式块）
#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// 输入的 Markdown 文件路径
    #[arg(value_name = "INPUT")]
    pub input: String,

    /// 输出的 LaTeX 文件路径（可选，默认替换 .md 为 .tex）
    #[arg(value_name = "OUTPUT")]
    pub output: Option<String>,
}

fn main() -> std::io::Result<()> {
    let args = Args::parse();

    let input_path = args.input;
    let output_path = args.output.unwrap_or_else(|| {
        let path = Path::new(&input_path);
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let parent = path.parent().unwrap_or_else(|| Path::new(""));
        parent
            .join(format!("{stem}.tex"))
            .to_string_lossy()
            .into_owned()
    });

    let md_content = std::fs::read_to_string(&input_path)?;
    let latex = convert_markdown_to_latex(&md_content);
    std::fs::write(&output_path, latex)?;

    Ok(())
}

#[derive(PartialEq, Eq)]
enum CustomCodeBlockKind<'a> {
    None,
    Code(Option<CowStr<'a>>),
    RawLatex,
    Equation,
}

fn convert_markdown_to_latex(markdown: &str) -> String {
    let preprocessed = preprocess(markdown);

    let mut output = String::new();
    let parser = MdParser::new_ext(&preprocessed, Options::all());

    let mut inside_image = false;
    let mut image_url = String::new();
    let mut image_caption = String::new();
    let mut _inside_paragraph = false;
    let mut _in_ordered_list = false;
    let mut _in_unordered_list = false;
    let mut codeblock_status = CustomCodeBlockKind::None;

    let mut _inside_header = false;
    let mut first_cell = true;

    let mut is_add_heading_to_contents = false;
    let mut heading_content_string: Option<String> = None;

    for event in parser {
        match event {
            Event::Start(Tag::Table(alignments)) => {
                // 构建列格式
                let column_format = alignments
                    .iter()
                    .map(|align| match align {
                        pulldown_cmark::Alignment::Left => r#">{\raggedright\arraybackslash}X"#,
                        pulldown_cmark::Alignment::Center => r#">{\centering\arraybackslash}X"#,
                        pulldown_cmark::Alignment::Right => r#">{\raggedleft\arraybackslash}X"#,
                        _ => r#">{\centering\arraybackslash}X"#, // fallback
                    })
                    .map(|s| format!("|{}", s))
                    .collect::<String>()
                    + "|";

                let text = format!(
                    "\\begin{{tabularx}}{{\\textwidth}}{{{}}} \\hline\n",
                    column_format
                );
                output.push_str(&text);
            }
            Event::End(Tag::Table(_)) => {
                output.push_str("\\end{tabularx}\n\n");
            }
            Event::Start(Tag::TableHead) => {
                _inside_header = true;
                first_cell = true;
            }
            Event::End(Tag::TableHead) => {
                output.push_str(" \\\\ \\hline\n");
            }
            Event::Start(Tag::TableRow) => {
                first_cell = true;
            }
            Event::End(Tag::TableRow) => {
                output.push_str(" \\\\ \\hline\n");
            }
            Event::Start(Tag::TableCell) => {
                if !first_cell {
                    output.push_str(" & ");
                }
                first_cell = false;
            }
            Event::End(Tag::TableCell) => {}

            Event::Start(Tag::Heading(level, _, classes)) => {
                output.push_str("\\");
                let mut base_string = match level {
                    HeadingLevel::H1 => "chapter",
                    HeadingLevel::H2 => "section",
                    HeadingLevel::H3 => "subsection",
                    HeadingLevel::H4 => "subsubsection",
                    HeadingLevel::H5 => "paragraph",
                    _ => "textbf",
                }
                .to_string();
                if classes.contains(&"unnumbered") {
                    base_string.push_str("*");
                }
                if classes.contains(&"add-contents") {
                    is_add_heading_to_contents = true;
                }
                base_string.push_str("{");
                output.push_str(&base_string);
            }
            Event::End(Tag::Heading(_, _, _)) => {
                output.push_str("}\n");
                if is_add_heading_to_contents {
                    output.push_str(&format!(
                        "\\addcontentsline{{toc}}{{chapter}}{{{}}}\n",
                        heading_content_string.as_ref().unwrap()
                    ));
                    is_add_heading_to_contents = false;
                    heading_content_string = None;
                }
                output.push_str("\n");
            }
            Event::Start(Tag::Paragraph) => {
                _inside_paragraph = true;
                // 可选：插入 \par 或开头标记
                // output.push_str("\\par\n");
            }
            Event::End(Tag::Paragraph) => {
                _inside_paragraph = false;
                output.push_str("\n\n"); // 两个换行表示段落结束
            }
            Event::Text(text) if inside_image => {
                image_caption.push_str(&text); // 累加文字，避免分段
            }
            Event::Text(text) if codeblock_status != CustomCodeBlockKind::None => {
                output.push_str(&text);
            }
            Event::Text(text) => {
                let replaced = apply_text_replacements(&text, &IN_TEXT_REPLACEMENT_TABLE);

                if is_add_heading_to_contents {
                    heading_content_string = Some(replaced.to_string())
                }
                output.push_str(&replaced);
            }
            Event::Start(Tag::Emphasis) => output.push_str("\\textit{"),
            Event::End(Tag::Emphasis) => output.push('}'),
            Event::Start(Tag::Strong) => output.push_str("\\textbf{"),
            Event::End(Tag::Strong) => output.push('}'),
            Event::Start(Tag::Link(_href, url, _)) => {
                output.push_str(&format!("\\href{{{}}}{{", url));
            }
            Event::End(Tag::Link(_, _, _)) => output.push('}'),
            Event::Start(Tag::Image(_, url, _)) => {
                inside_image = true;
                image_url = url.to_string();
            }
            Event::End(Tag::Image(_, _, _)) => {
                output.push_str("\\begin{figure}[htbp]\n");
                output.push_str(&format!(
                    "\\centering\n\\includegraphics[width=0.8\\textwidth]{{{}}}\n",
                    image_url
                ));
                output.push_str(&format!("\\caption{{{}}}\n", image_caption));
                output.push_str(&format!("\\label{{fig:{}}}\n", image_url));
                output.push_str("\\end{figure}\n");

                inside_image = false;
                image_url.clear();
                image_caption.clear();
            }
            Event::Start(Tag::List(Some(_))) => {
                output.push_str("\\begin{enumerate}\n");
                _in_ordered_list = true;
            }
            Event::Start(Tag::List(None)) => {
                output.push_str("\\begin{itemize}\n");
                _in_unordered_list = true;
            }
            Event::End(Tag::List(Some(_))) => {
                output.push_str("\\end{enumerate}\n");
                _in_ordered_list = false;
            }
            Event::End(Tag::List(None)) => {
                output.push_str("\\end{itemize}\n");
                _in_unordered_list = false;
            }
            Event::Start(Tag::Item) => {
                output.push_str("\\item ");
            }
            Event::End(Tag::Item) => {
                output.push('\n');
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                codeblock_status = handle_code_block_start(kind, &mut output);
            }
            Event::End(Tag::CodeBlock(_)) => {
                match codeblock_status {
                    CustomCodeBlockKind::None => {}
                    CustomCodeBlockKind::Code(_) => {
                        output.push_str("\\end{lstlisting}\n\n");
                    }
                    CustomCodeBlockKind::RawLatex => {}
                    CustomCodeBlockKind::Equation => {
                        output.push_str("\\end{equation}\n\n");
                    }
                }

                codeblock_status = CustomCodeBlockKind::None;
            }
            Event::Code(code) => {
                // 行内代码可映射为 \texttt{}
                output.push_str(&format!(
                    "\\texttt{{{}}}",
                    apply_text_replacements(&code, &IN_TEXT_REPLACEMENT_TABLE)
                ));
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

    let postprocessed = postprocess(&mut output);

    postprocessed
}

fn preprocess(input: &str) -> String {
    // let input = apply_text_replacements(input, &PRE_REPLACEMENT_TABLE);
    let mut output = String::new();
    let mut lines = input.lines().peekable();

    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.starts_with("$$") {
            // 开始公式块
            let label = trimmed.trim_start_matches("$$").trim();
            let mut content = String::new();

            // 收集中间内容
            while let Some(next_line) = lines.peek() {
                if next_line.trim() == "$$" {
                    lines.next(); // consume closing
                    break;
                } else {
                    content.push_str(&lines.next().unwrap());
                    content.push('\n');
                }
            }

            if label.is_empty() {
                output.push_str("``` block_equation{}\n");
            } else {
                output.push_str(&format!("``` block_equation{}\n", label));
            }
            output.push_str(&content);
            output.push_str("```\n");
        } else if trimmed.starts_with("```latex raw") {
            output.push_str(line);
            output.push('\n');

            while let Some(next_line) = lines.peek() {
                if next_line.trim() == "```" {
                    output.push_str(lines.next().unwrap());
                    output.push('\n');
                } else {
                    output.push_str(lines.next().unwrap());
                    output.push('\n');
                }
            }
        } else {
            output.push_str(&apply_text_replacements(&line, &PRE_REPLACEMENT_TABLE));
            output.push('\n');
        }
    }

    output
}

/// 对纯文本做替换
fn apply_text_replacements(text: &str, table: &HashMap<&str, &str>) -> String {
    let mut replaced = text.to_string();
    for (md, latex) in table {
        replaced = replaced.replace(md, latex);
    }
    replaced
}

fn apply_text_replacements_inversedly(text: &str, table: &HashMap<&str, &str>) -> String {
    let mut replaced = text.to_string();
    for (md, latex) in table {
        replaced = replaced.replace(latex, md);
    }
    replaced
}

fn handle_code_block_start<'a>(
    kind: CodeBlockKind<'a>,
    output: &mut String,
) -> CustomCodeBlockKind<'a> {
    if let CodeBlockKind::Fenced(ref lang) = kind {
        let str = lang.clone();
        let tags = str
            .split(" ")
            .filter(|it| !it.is_empty())
            .collect::<Vec<_>>();
        for tag in tags.iter() {
            // Equation Matching
            let reg = Regex::new(r"block_equation\{(.*?)\}").unwrap();
            for caps in reg.captures_iter(tag) {
                output.push_str("\\begin{equation}\n");
                let name = &caps[1];
                if !name.is_empty() {
                    output.push_str(&format!("\\label{{eq:{}}}\n", name));
                }
                return CustomCodeBlockKind::Equation;
            }
        }
        if tags.contains(&"latex") && tags.contains(&"raw") {
            CustomCodeBlockKind::RawLatex
        } else {
            output.push_str("\\begin{lstlisting}\n");
            CustomCodeBlockKind::Code(Some(lang.clone()))
        }
    } else {
        CustomCodeBlockKind::Code(None)
    }
}

fn postprocess(input: &mut String) -> String {
    let re = Regex::new(r#"\\cite\\\{(.*?)\\\}"#).unwrap();
    let result = re.replace_all(input, r"\cite{$1}");

    let re = Regex::new(r#"\\ref\\\{(.*?)\\\}"#).unwrap();
    let result = re.replace_all(&result, r"\ref{$1}");

    let result = inversedly_replace(&result, r"\$([^\$\n]+)\$", |it| format!("${}$", it));
    let result = inversedly_replace(&result, r"\\ref\{([^}]+)\}", |it| {
        format!("\\ref{{{}}}", it)
    });
    result.to_string()
}

fn inversedly_replace<'a>(
    input: &'a str,
    regex: &str,
    formater: impl Fn(&str) -> String,
) -> Cow<'a, str> {
    let re = Regex::new(regex).unwrap();
    re.replace_all(input, |caps: &regex::Captures| {
        let inner = &caps[1];
        let rp = apply_text_replacements_inversedly(inner, &IN_TEXT_REPLACEMENT_TABLE);
        formater(&rp)
    })
}
