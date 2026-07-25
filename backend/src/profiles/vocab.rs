/// One selectable value. Served to the frontend so the form's labels and the
/// database's CHECK constraints can never drift apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Choice {
    pub id: &'static str,
    pub label: &'static str,
}

pub const ROLES: [Choice; 6] = [
    Choice {
        id: "engineering",
        label: "Engineering",
    },
    Choice {
        id: "design",
        label: "Design",
    },
    Choice {
        id: "product",
        label: "Product",
    },
    Choice {
        id: "gtm",
        label: "GTM / Sales",
    },
    Choice {
        id: "ops_finance",
        label: "Ops / Finance",
    },
    Choice {
        id: "research",
        label: "Research / Science",
    },
];

pub const IDEA_STATUSES: [Choice; 3] = [
    Choice {
        id: "committed_idea",
        label: "I have an idea I'm committed to",
    },
    Choice {
        id: "flexible_idea",
        label: "I have an idea but I'm flexible",
    },
    Choice {
        id: "looking_to_join",
        label: "I'm looking to join someone else's",
    },
];

pub const STAGES: [Choice; 4] = [
    Choice {
        id: "idea",
        label: "Idea",
    },
    Choice {
        id: "prototype",
        label: "Prototype",
    },
    Choice {
        id: "users",
        label: "Users",
    },
    Choice {
        id: "revenue",
        label: "Revenue",
    },
];

pub const COMMITMENTS: [Choice; 4] = [
    Choice {
        id: "full_time_now",
        label: "Full-time now",
    },
    Choice {
        id: "full_time_when_funded",
        label: "Full-time once funded",
    },
    Choice {
        id: "part_time",
        label: "Part-time",
    },
    Choice {
        id: "exploring",
        label: "Exploring",
    },
];

pub const INTERESTS: [Choice; 18] = [
    Choice {
        id: "ai_ml",
        label: "AI / ML",
    },
    Choice {
        id: "agritech",
        label: "Agriculture",
    },
    Choice {
        id: "biotech",
        label: "Biotech",
    },
    Choice {
        id: "climate",
        label: "Climate",
    },
    Choice {
        id: "consumer_social",
        label: "Consumer / Social",
    },
    Choice {
        id: "developer_tools",
        label: "Developer tools",
    },
    Choice {
        id: "ecommerce",
        label: "E-commerce",
    },
    Choice {
        id: "edtech",
        label: "Education",
    },
    Choice {
        id: "fintech",
        label: "Fintech",
    },
    Choice {
        id: "gaming",
        label: "Gaming",
    },
    Choice {
        id: "healthtech",
        label: "Health",
    },
    Choice {
        id: "logistics",
        label: "Logistics",
    },
    Choice {
        id: "marketplace",
        label: "Marketplaces",
    },
    Choice {
        id: "media",
        label: "Media",
    },
    Choice {
        id: "real_estate",
        label: "Real estate",
    },
    Choice {
        id: "robotics",
        label: "Robotics",
    },
    Choice {
        id: "saas",
        label: "SaaS",
    },
    Choice {
        id: "security",
        label: "Security",
    },
];

pub fn contains(choices: &[Choice], id: &str) -> bool {
    choices.iter().any(|choice| choice.id == id)
}

pub fn label(choices: &[Choice], id: &str) -> Option<&'static str> {
    choices
        .iter()
        .find(|choice| choice.id == id)
        .map(|choice| choice.label)
}
