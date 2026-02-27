//! Category tabs component

use leptos::prelude::*;

/// Category type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Main,
    Mics,
    Stems,
    Tech,
}

impl Category {
    pub fn label(&self) -> &'static str {
        match self {
            Category::Main => "Main",
            Category::Mics => "Mics",
            Category::Stems => "Stems",
            Category::Tech => "Tech",
        }
    }

    pub fn matches(&self, category: &str) -> bool {
        match self {
            Category::Main => false, // Main has special rendering, not category filtering
            Category::Mics => category == "mics",
            Category::Stems => category == "stems",
            Category::Tech => category == "tech",
        }
    }

    pub fn class(&self) -> &'static str {
        match self {
            Category::Main => "main",
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
        Category::Main,
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
