use crate::profiles::vocab::Choice;

/// Served by `GET /options` alongside the profile vocabularies, so the
/// report form's wording and the database's CHECK constraint cannot drift.
pub const REPORT_REASONS: [Choice; 5] = [
    Choice {
        id: "harassment",
        label: "Harassment or abuse",
    },
    Choice {
        id: "spam",
        label: "Spam or advertising",
    },
    Choice {
        id: "impersonation",
        label: "Impersonation or a fake profile",
    },
    Choice {
        id: "off_topic",
        label: "Not here to find a cofounder",
    },
    Choice {
        id: "other",
        label: "Something else",
    },
];
