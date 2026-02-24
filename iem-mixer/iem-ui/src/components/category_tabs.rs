//! Category tabs component

use leptos::prelude::*;

/// Category type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    All,
    Mics,
    Stems,
    Tech,
}

impl Category {
    pub fn label(&self) -> &'static str {
        match self {
            Category::All => "All",
            Category::Mics => "Mics",
            Category::Stems => "Stems",
            Category::Tech => "Tech",
        }
    }

    pub fn matches(&self, category: &str) -> bool {
        match self {
            Category::All => true,
            Category::Mics => category == "mics",
            Category::Stems => category == "stems",
            Category::Tech => category == "tech",
        }
    }

    pub fn class(&self) -> &'static str {
        match self {
            Category::All => "all",
            Category::Mics => "mics",
            Category::Stems => "stems",
            Category::Tech => "tech",
        }
    }
}

/// Category tabs component
#[component]
pub fn CategoryTabs(
    /// Currently selected category
    active: ReadSignal<Category>,
    /// Called when category changes
    on_select: impl Fn(Category) + 'static + Clone,
) -> impl IntoView {
    let categories = [
        Category::All,
        Category::Mics,
        Category::Stems,
        Category::Tech,
    ];

    view! {
        <div class="category-tabs">
            {categories.into_iter().map(|cat| {
                let on_select = on_select.clone();
                view! {
                    <button
                        class=move || {
                            let base = format!("category-tab {}", cat.class());
                            if active.get() == cat {
                                format!("{} active", base)
                            } else {
                                base
                            }
                        }
                        on:click=move |_| on_select(cat)
                    >
                        {cat.label()}
                    </button>
                }
            }).collect::<Vec<_>>()}
        </div>
    }
}
