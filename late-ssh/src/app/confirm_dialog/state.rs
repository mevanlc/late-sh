#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfirmDialogState {
    pub title: String,
    pub prompt: String,
    pub detail: String,
    pub required_text: Option<String>,
    pub input_value: String,
    pub confirm_label: String,
    pub cancel_label: String,
}

impl ConfirmDialogState {
    pub fn new(
        title: impl Into<String>,
        prompt: impl Into<String>,
        detail: impl Into<String>,
        required_text: Option<String>,
        confirm_label: impl Into<String>,
        cancel_label: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            prompt: prompt.into(),
            detail: detail.into(),
            required_text,
            input_value: String::new(),
            confirm_label: confirm_label.into(),
            cancel_label: cancel_label.into(),
        }
    }

    pub fn typed(
        title: impl Into<String>,
        prompt: impl Into<String>,
        detail: impl Into<String>,
        required_text: impl Into<String>,
        confirm_label: impl Into<String>,
        cancel_label: impl Into<String>,
    ) -> Self {
        Self::new(
            title,
            prompt,
            detail,
            Some(required_text.into()),
            confirm_label,
            cancel_label,
        )
    }

    pub fn push(&mut self, ch: char) {
        self.input_value.push(ch);
    }

    pub fn backspace(&mut self) {
        self.input_value.pop();
    }

    pub fn delete_word_left(&mut self) {
        while self.input_value.ends_with(char::is_whitespace) {
            self.input_value.pop();
        }
        while self
            .input_value
            .chars()
            .last()
            .is_some_and(|ch| !ch.is_whitespace())
        {
            self.input_value.pop();
        }
    }

    pub fn is_confirm_enabled(&self) -> bool {
        match &self.required_text {
            Some(required) => self.input_value.trim() == required.trim(),
            None => true,
        }
    }
}
