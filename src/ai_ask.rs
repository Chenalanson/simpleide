//! Parsing and drafting answers for `se-ask` clarification cards.
//!
//! The model emits a question plus optional choices. The chat renders a card so
//! the user can pick a choice or type a custom answer; the answer is then sent
//! as the next user message.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AiAsk {
    pub question: String,
    pub options: Vec<String>,
    pub multiple: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct AiAskDraft {
    pub selected: Vec<bool>,
    pub custom: String,
}

impl AiAskDraft {
    pub fn new(ask: &AiAsk) -> Self {
        Self {
            selected: vec![false; ask.options.len()],
            custom: String::new(),
        }
    }
}

pub fn parse_ask(attrs: &str, body: &str) -> AiAsk {
    let multiple = attrs.split_whitespace().any(|attr| {
        let attr = attr.trim_matches(|ch| matches!(ch, ',' | ';'));
        attr.eq_ignore_ascii_case("multiple") || attr.eq_ignore_ascii_case("multi")
    });
    let mut question_lines = Vec::new();
    let mut options = Vec::new();
    let mut in_options = false;
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(option) = option_text(trimmed) {
            in_options = true;
            if options.len() < 8
                && !options
                    .iter()
                    .any(|existing: &String| existing.eq_ignore_ascii_case(&option))
            {
                options.push(truncate(&option, 200));
            }
        } else if !in_options {
            question_lines.push(trimmed.to_owned());
        }
    }
    let question = truncate(&question_lines.join("\n"), 800);
    let error = question
        .is_empty()
        .then(|| "se-ask 缺少问题文本".to_owned());
    AiAsk {
        question,
        options,
        multiple,
        error,
    }
}

/// Fallback for models that ask a confirmation question in prose instead of
/// emitting a `se-ask` block. Only a narrow set of explicit permission /
/// decision phrases is converted, so normal explanations do not become cards.
pub fn fallback_question_card(text: &str) -> Option<AiAsk> {
    const TRIGGERS: &[&str] = &[
        "请允许",
        "是否允许",
        "需要你确认",
        "请确认",
        "请选择",
        "是否继续",
        "是否需要",
        "要不要",
        "是否替换",
        "是否重构",
        "请回复",
        "请告诉我",
    ];
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut candidate = None;
    for paragraph in trimmed.split("\n\n") {
        if TRIGGERS.iter().any(|trigger| paragraph.contains(trigger)) {
            candidate = Some(paragraph.trim());
        }
    }
    let candidate = candidate?;
    let question = truncate(candidate, 800);
    let options = if question.contains("允许") {
        vec!["允许", "不允许"]
    } else if question.contains("替换") {
        vec!["替换", "保留"]
    } else if question.contains("重构") {
        vec!["重构", "暂不重构"]
    } else if question.contains("继续") {
        vec!["继续", "暂停"]
    } else {
        vec!["是", "否"]
    };
    Some(AiAsk {
        question,
        options: options
            .into_iter()
            .map(|option| option.to_owned())
            .collect(),
        multiple: false,
        error: None,
    })
}

pub fn answers_complete(asks: &[AiAsk], drafts: &[AiAskDraft]) -> bool {
    asks.len() == drafts.len()
        && asks.iter().zip(drafts).all(|(ask, draft)| {
            ask.error.is_none()
                && (draft.selected.iter().any(|selected| *selected)
                    || !draft.custom.trim().is_empty())
        })
}

/// Build the user message sent after the user answers all questions. Returns
/// `None` while any question is still unanswered.
pub fn answer_text(asks: &[AiAsk], drafts: &[AiAskDraft]) -> Option<String> {
    if asks.is_empty() || asks.len() != drafts.len() {
        return None;
    }
    let mut blocks = Vec::new();
    for (index, (ask, draft)) in asks.iter().zip(drafts).enumerate() {
        if ask.error.is_some() {
            return None;
        }
        let selected = ask
            .options
            .iter()
            .zip(&draft.selected)
            .filter(|(_, selected)| **selected)
            .map(|(option, _)| option.clone())
            .collect::<Vec<_>>();
        let custom = draft.custom.trim();
        if selected.is_empty() && custom.is_empty() {
            return None;
        }
        let mut lines = vec![format!(
            "{}. {}",
            index + 1,
            ask.question.replace('\n', " ")
        )];
        if !selected.is_empty() {
            lines.push(format!("   选择：{}", selected.join("、")));
        }
        if !custom.is_empty() {
            lines.push(format!("   自定义：{}", truncate(custom, 2_000)));
        }
        blocks.push(lines.join("\n"));
    }
    Some(format!("回答你的确认问题：\n\n{}", blocks.join("\n\n")))
}

fn option_text(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let rest = ["- ", "* ", "• ", "+ "]
        .iter()
        .find_map(|prefix| trimmed.strip_prefix(prefix))
        .or_else(|| {
            let digits = trimmed.chars().take_while(|ch| ch.is_ascii_digit()).count();
            if digits == 0 {
                return None;
            }
            let after = &trimmed[digits..];
            after
                .strip_prefix(". ")
                .or_else(|| after.strip_prefix(") "))
        })?;
    let rest = rest.trim();
    (!rest.is_empty()).then(|| rest.to_owned())
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let mut result = text.chars().take(limit).collect::<String>();
    result.push('…');
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_question_and_bullet_options() {
        let ask = parse_ask("", "使用哪个串口？\n\n- USART1\n- USART2\n- USART3\n");
        assert_eq!(ask.question, "使用哪个串口？");
        assert_eq!(ask.options, vec!["USART1", "USART2", "USART3"]);
        assert!(!ask.multiple);
        assert!(ask.error.is_none());
    }

    #[test]
    fn parses_multiple_and_numbered_options() {
        let ask = parse_ask(
            "multiple",
            "需要保留哪些功能？\n1. 串口回显\n2) DMA\n- 空闲中断\n",
        );
        assert!(ask.multiple);
        assert_eq!(ask.options, vec!["串口回显", "DMA", "空闲中断"]);
    }

    #[test]
    fn allows_free_form_question_without_options() {
        let ask = parse_ask("", "目标芯片的具体型号是什么？\n");
        assert_eq!(ask.question, "目标芯片的具体型号是什么？");
        assert!(ask.options.is_empty());
        assert!(ask.error.is_none());
    }

    #[test]
    fn deduplicates_options_case_insensitively() {
        let ask = parse_ask("", "选择串口？\n- USART1\n- usart1\n- USART2\n");
        assert_eq!(ask.options, vec!["USART1", "USART2"]);
    }

    #[test]
    fn builds_answer_with_selection_and_custom_text() {
        let asks = vec![
            parse_ask("", "使用哪个串口？\n- USART1\n- USART2\n"),
            parse_ask("multiple", "保留哪些功能？\n- DMA\n- 空闲中断\n"),
        ];
        let mut drafts = asks.iter().map(AiAskDraft::new).collect::<Vec<_>>();
        drafts[0].selected[0] = true;
        drafts[1].selected[0] = true;
        drafts[1].selected[1] = true;
        drafts[1].custom = "保留调试串口".to_owned();
        assert!(answers_complete(&asks, &drafts));
        let answer = answer_text(&asks, &drafts).unwrap();
        assert!(answer.contains("使用哪个串口？"));
        assert!(answer.contains("选择：USART1"));
        assert!(answer.contains("选择：DMA、空闲中断"));
        assert!(answer.contains("自定义：保留调试串口"));
    }

    #[test]
    fn reports_incomplete_answers() {
        let asks = vec![parse_ask("", "使用哪个串口？\n- USART1\n- USART2\n")];
        let drafts = asks.iter().map(AiAskDraft::new).collect::<Vec<_>>();
        assert!(!answers_complete(&asks, &drafts));
        assert!(answer_text(&asks, &drafts).is_none());
    }

    #[test]
    fn reports_missing_question() {
        let ask = parse_ask("", "- USART1\n- USART2\n");
        assert!(ask.error.is_some());
        assert_eq!(ask.options.len(), 2);
    }

    #[test]
    fn fallback_card_for_permission_question() {
        let text =
            "在收尾前还有一处需要确认：\n\n请允许我读取两份真实正文确认：\n\n拿到后我会继续。";
        let ask = fallback_question_card(text).expect("question card");
        assert!(ask.question.contains("请允许我读取"));
        assert_eq!(ask.options, vec!["允许", "不允许"]);
        assert!(ask.error.is_none());
    }

    #[test]
    fn fallback_card_ignores_plain_explanation() {
        assert!(fallback_question_card("本次修改已完成，构建通过。").is_none());
        assert!(fallback_question_card("这里只是一个普通说明，没有等待用户决定。").is_none());
    }
}
