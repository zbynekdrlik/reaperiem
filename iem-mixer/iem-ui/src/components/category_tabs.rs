//! Category tabs component

use leptos::prelude::*;

/// Category type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Main,
    Mics,
    Stems,
    Tech,
    Hidden,
}

impl Category {
    pub fn label(&self) -> &'static str {
        match self {
            Category::Main => "Main",
            Category::Mics => "Mics",
            Category::Stems => "Stems",
            Category::Tech => "Tech",
            Category::Hidden => "Hidden",
        }
    }

    pub fn matches(&self, category: &str) -> bool {
        match self {
            Category::Main => false,   // Main has special rendering
            Category::Hidden => false, // Hidden has special rendering
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
            Category::Hidden => "hidden",
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
    /// Whether to show the Hidden tab (only when there are hidden channels)
    #[prop(default = false.into())]
    show_hidden: Signal<bool>,
) -> impl IntoView {
    let base_categories = [
        Category::Main,
        Category::Mics,
        Category::Stems,
        Category::Tech,
    ];

    view! {
        <div class="category-tabs">
            {base_categories.into_iter().map(|cat| {
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
            <Show
                when=move || show_hidden.get()
                fallback=|| ()
            >
                {
                    let on_select = on_select.clone();
                    view! {
                        <button
                            class=move || {
                                let base = "category-tab hidden".to_string();
                                if active.get() == Category::Hidden {
                                    format!("{} active", base)
                                } else {
                                    base
                                }
                            }
                            on:click=move |_| on_select(Category::Hidden)
                        >
                            "Hidden"
                        </button>
                    }
                }
            </Show>
        </div>
    }
}
