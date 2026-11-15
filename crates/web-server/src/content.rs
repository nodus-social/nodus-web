//! What a status body becomes before it is painted.
//!
//! Sanitisation and custom-emoji expansion live in [`nodus_ui::html::prepared`] so the
//! server paint and the browser mount share the exact same allowlist and text-node emoji
//! traversal.

/// Sanitises a status body and paints the emoji the status itself declares.
#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "shared StatusBody now runs nodus_ui::html::prepared directly"
    )
)]
pub fn prepared(content: &str, emojis: &[nodus_client_core::model::CustomEmoji]) -> String {
    nodus_ui::html::prepared(content, emojis)
}

#[cfg(test)]
mod tests {
    use nodus_client_core::model::CustomEmoji;

    use super::prepared;

    #[test]
    fn prepared_does_not_break_out_of_href_attributes() {
        let emoji = CustomEmoji {
            shortcode: "wave".to_owned(),
            url: "https://cdn.test/wave.png".to_owned(),
            static_url: None,
        };
        let html = prepared(
            "<p><a href=\"https://example.com/:wave:\">hi :wave:</a></p><img src=x onerror=alert(1)>",
            &[emoji],
        );
        assert!(
            html.contains("href=\"https://example.com/:wave:\""),
            "{html}"
        );
        assert!(!html.contains("onerror"), "{html}");
        assert_eq!(
            html.matches("src=\"https://cdn.test/wave.png\"").count(),
            1,
            "{html}"
        );
    }
}
