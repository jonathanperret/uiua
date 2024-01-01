use leptos::*;
use leptos_meta::*;
use leptos_router::*;

use crate::markdown::*;

const SITE_TITLE: &str = "Advent of Code 2023 avec Uiua";

#[derive(Debug, Clone, PartialEq, Eq, Params)]
pub struct BlogParams {
    page: BlogParam,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BlogParam(String);
impl IntoParam for BlogParam {
    fn into_param(value: Option<&str>, _: &str) -> Result<Self, ParamsError> {
        let s = value.unwrap_or_default();
        let name = urlencoding::decode(s)
            .map(Into::into)
            .unwrap_or_else(|_| s.into());
        Ok(BlogParam(name))
    }
}

#[component]
pub fn Blog() -> impl IntoView {
    view!({
        move || match use_params::<BlogParams>().get() {
            Ok(params) => {
                if params.page.0.is_empty() {
                    view!(<BlogHome/>)
                } else {
                    view!(<BlogPage name={params.page.0}/>)
                }
            }
            Err(_) => view!(<BlogHome/>),
        }
    })
}

#[component]
fn BlogHome() -> impl IntoView {
    view! {
        <Title text={SITE_TITLE}/>
        <h1>{SITE_TITLE}</h1>
        <Fetch src="/blog/list.txt" f=|list| {
            list.lines().map(|name| {
                let (path, name) = name.split_once(": ").unwrap_or_default();
                let (date, name) = name.split_once(" - ").unwrap_or_default();
                let name = name.to_string();
                let date = date.to_string();
                view!(<h3><span class="output-faint">{date}" - "</span><A href={format!("/blog/{path}")}>{name}</A></h3>)
            }).collect::<Vec<_>>().into_view()
        }/>
    }
}

#[component]
fn BlogPage(name: String) -> impl IntoView {
    view! {
        <Title text={format!("{name} - {SITE_TITLE}")}/>
        <A href="/">"Retour"</A>
        <br/>
        <br/>
        <Markdown src={format!("/blog/{name}-text.md")}/>
        <br/>
        <br/>
        <A href="/">"Retour"</A>
    }
}
