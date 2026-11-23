//! What a settings read says when the instance refuses it.
//!
//! A refused grant is not a broken server: the SDK reports a 403 as
//! [`nodus_client_core::Error::Unauthorized`] ("token rejected"), and repeating that
//! sentence would leave the reader looking for an outage. The scope it names is the
//! thing the reader can act on, so it is named here instead.

/// The sentence shown for one refused read.
///
/// A capability refusal already has the server's own statement of what it does not
/// serve, so that wins. A refused grant gets the sentence above; anything else is more
/// specific than this screen can be and keeps its message.
#[must_use]
pub fn sentence(error: &str) -> String {
    let explained = crate::state::bridge::capabilities::explained(error);
    if explained != error {
        return explained;
    }
    if error.contains("token rejected") {
        "The instance refused this with your current grant: the token was revoked, or it \
         lacks a scope this screen needs. Signing in again asks for the scopes it needs."
            .to_owned()
    } else {
        error.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::sentence;

    #[test]
    fn a_refused_grant_names_the_grant_not_the_server() {
        let answer = sentence("token rejected (revoked or insufficient scope)");
        assert!(answer.contains("grant"));
        assert!(!answer.contains("token rejected"));
    }

    #[test]
    fn any_other_refusal_keeps_the_instances_own_reason() {
        assert_eq!(
            sentence("instance returned 503: unavailable"),
            "instance returned 503: unavailable"
        );
    }
}
