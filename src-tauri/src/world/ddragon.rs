//! Imagens e nomes oficiais do League of Legends pelo Data Dragon (servidor público da
//! Riot): campeões, itens, runas e feitiços, em português e em inglês (os sites de builds
//! usam os nomes em inglês). Baixado uma vez por sessão e guardado na memória.

use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;

const CDN: &str = "https://ddragon.leagueoflegends.com/cdn";

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Icon {
    /// Nome em português (aparece ao passar o mouse).
    pub name: String,
    pub url: String,
}

#[derive(Default)]
pub struct Dd {
    pub version: String,
    /// id numérico -> (chave da imagem, nome)
    champions: HashMap<i64, (String, String)>,
    /// (nomes em inglês e português sem acento) -> ícone
    items: Vec<(String, String, Icon)>,
    runes: Vec<(String, String, Icon)>,
    spells: Vec<(String, String, Icon)>,
}

static DATA: tokio::sync::Mutex<Option<Arc<Dd>>> = tokio::sync::Mutex::const_new(None);

/// Letras e números, minúsculas e sem acento ("Doran's Blade" -> "doransblade").
pub fn fold(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'ê' | 'è' => 'e',
            'í' | 'î' => 'i',
            'ó' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ü' => 'u',
            'ç' => 'c',
            other => other,
        })
        .filter(|c| c.is_alphanumeric())
        .collect()
}

async fn get(url: String) -> Option<Value> {
    crate::world::http().get(url).send().await.ok()?.json().await.ok()
}

fn runes_of(tree_list: &Value) -> Vec<(i64, String, String)> {
    let mut out = Vec::new();
    for tree in tree_list.as_array().into_iter().flatten() {
        if let (Some(id), Some(name), Some(icon)) = (tree["id"].as_i64(), tree["name"].as_str(), tree["icon"].as_str()) {
            out.push((id, name.to_string(), icon.to_string()));
        }
        for slot in tree["slots"].as_array().into_iter().flatten() {
            for rune in slot["runes"].as_array().into_iter().flatten() {
                if let (Some(id), Some(name), Some(icon)) = (rune["id"].as_i64(), rune["name"].as_str(), rune["icon"].as_str()) {
                    out.push((id, name.to_string(), icon.to_string()));
                }
            }
        }
    }
    out
}

async fn load() -> Option<Dd> {
    let versions = get("https://ddragon.leagueoflegends.com/api/versions.json".to_string()).await?;
    let v = versions.as_array()?.first()?.as_str()?.to_string();
    let data = |lang: &str, file: &str| format!("{CDN}/{v}/data/{lang}/{file}");
    let (champs, items_en, items_pt, runes_en, runes_pt, spells_en, spells_pt) = futures_util::join!(
        get(data("pt_BR", "champion.json")),
        get(data("en_US", "item.json")),
        get(data("pt_BR", "item.json")),
        get(data("en_US", "runesReforged.json")),
        get(data("pt_BR", "runesReforged.json")),
        get(data("en_US", "summoner.json")),
        get(data("pt_BR", "summoner.json")),
    );
    let mut dd = Dd { version: v.clone(), ..Default::default() };
    for (key, c) in champs?["data"].as_object()? {
        if let (Some(id), Some(name)) = (c["key"].as_str().and_then(|k| k.parse().ok()), c["name"].as_str()) {
            dd.champions.insert(id, (key.clone(), name.to_string()));
        }
    }
    if let (Some(en), Some(pt)) = (items_en, items_pt) {
        // Só itens de Summoner's Rift (mapa 11); com nomes repetidos (Arena, Ornn…), fica o de menor id.
        let mut list: Vec<(&String, &Value)> = en["data"].as_object().into_iter().flatten().filter(|(_, item)| item["maps"]["11"] == true).collect();
        list.sort_by_key(|(id, _)| id.parse::<u64>().unwrap_or(u64::MAX));
        for (id, item) in list {
            let name_en = item["name"].as_str().unwrap_or("");
            if dd.items.iter().any(|(en, _, _)| *en == fold(name_en)) {
                continue;
            }
            let name_pt = pt["data"][id]["name"].as_str().unwrap_or(name_en);
            dd.items.push((fold(name_en), fold(name_pt), Icon { name: name_pt.to_string(), url: format!("{CDN}/{v}/img/item/{id}.png") }));
        }
    }
    if let (Some(en), Some(pt)) = (runes_en, runes_pt) {
        let pt_names: HashMap<i64, String> = runes_of(&pt).into_iter().map(|(id, name, _)| (id, name)).collect();
        for (id, name_en, icon) in runes_of(&en) {
            let name_pt = pt_names.get(&id).cloned().unwrap_or_else(|| name_en.clone());
            dd.runes.push((fold(&name_en), fold(&name_pt), Icon { name: name_pt, url: format!("{CDN}/img/{icon}") }));
        }
    }
    if let (Some(en), Some(pt)) = (spells_en, spells_pt) {
        // Só feitiços do modo normal (o Flash do Arena tem o mesmo nome).
        for (key, spell) in en["data"].as_object().into_iter().flatten() {
            if !spell["modes"].as_array().is_some_and(|m| m.iter().any(|x| x == "CLASSIC")) {
                continue;
            }
            let name_en = spell["name"].as_str().unwrap_or("");
            let name_pt = pt["data"][key]["name"].as_str().unwrap_or(name_en);
            let image = spell["image"]["full"].as_str().unwrap_or("");
            dd.spells.push((fold(name_en), fold(name_pt), Icon { name: name_pt.to_string(), url: format!("{CDN}/{v}/img/spell/{image}") }));
        }
    }
    Some(dd)
}

/// Dados do Data Dragon (baixados na primeira vez que precisa).
pub async fn data() -> Option<Arc<Dd>> {
    let mut guard = DATA.lock().await;
    if guard.is_none() {
        *guard = load().await.map(Arc::new);
    }
    guard.clone()
}

impl Dd {
    pub fn champion_by_id(&self, id: i64) -> Option<Icon> {
        let (key, name) = self.champions.get(&id)?;
        Some(Icon { name: name.clone(), url: format!("{CDN}/{}/img/champion/{key}.png", self.version) })
    }

    /// Pelo nome (como o cliente em português mostra: "Wukong", "Kai'Sa").
    pub fn champion_by_name(&self, name: &str) -> Option<Icon> {
        let wanted = fold(name);
        let id = self.champions.iter().find(|(_, (key, n))| fold(n) == wanted || fold(key) == wanted).map(|(id, _)| *id)?;
        self.champion_by_id(id)
    }

    fn find(list: &[(String, String, Icon)], name: &str) -> Option<Icon> {
        let wanted = fold(name);
        if wanted.is_empty() {
            return None;
        }
        list.iter()
            .find(|(en, pt, _)| *en == wanted || *pt == wanted)
            .or_else(|| list.iter().find(|(en, pt, _)| en.starts_with(&wanted) || pt.starts_with(&wanted)))
            .map(|(_, _, icon)| icon.clone())
    }

    pub fn item(&self, name: &str) -> Option<Icon> {
        Self::find(&self.items, name)
    }

    pub fn rune(&self, name: &str) -> Option<Icon> {
        Self::find(&self.runes, name)
    }

    pub fn spell(&self, name: &str) -> Option<Icon> {
        // "Flash" -> "Flash"; os sites às vezes escrevem "Summoner Flash".
        Self::find(&self.spells, name.trim_start_matches("Summoner ").trim())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acha_por_nome_em_ingles_ou_portugues() {
        let icon = |n: &str| Icon { name: n.into(), url: format!("u/{n}") };
        let dd = Dd {
            version: "15.22.1".into(),
            champions: HashMap::from([(62, ("MonkeyKing".to_string(), "Wukong".to_string()))]),
            items: vec![(fold("Doran's Blade"), fold("Lâmina de Doran"), icon("Lâmina de Doran"))],
            runes: vec![(fold("Conqueror"), fold("Conquistador"), icon("Conquistador"))],
            spells: vec![(fold("Flash"), fold("Flash"), icon("Flash"))],
        };
        assert_eq!(dd.champion_by_name("wukong").unwrap().url, format!("{CDN}/15.22.1/img/champion/MonkeyKing.png"));
        assert_eq!(dd.item("Doran's Blade").unwrap().name, "Lâmina de Doran");
        assert_eq!(dd.item("Lamina de Doran").unwrap().name, "Lâmina de Doran");
        assert_eq!(dd.rune("conqueror").unwrap().name, "Conquistador");
        assert_eq!(dd.spell("Summoner Flash").unwrap().name, "Flash");
        assert!(dd.item("Espada inexistente").is_none());
    }
}

#[cfg(test)]
mod live {
    #[test]
    #[ignore = "baixa os dados do Data Dragon"]
    fn dados_reais() {
        let dd = tauri::async_runtime::block_on(super::data()).expect("sem Data Dragon");
        println!("versão {}", dd.version);
        for n in ["Wukong", "Kai'Sa", "Jinx"] {
            println!("campeão {n}: {:?}", dd.champion_by_name(n));
        }
        for n in ["Conqueror", "Legend: Alacrity", "Second Wind", "Electrocute"] {
            println!("runa {n}: {:?}", dd.rune(n));
        }
        for n in ["Doran's Blade", "Trinity Force", "Plated Steelcaps", "Health Potion"] {
            println!("item {n}: {:?}", dd.item(n));
        }
        for n in ["Flash", "Ignite"] {
            println!("feitiço {n}: {:?}", dd.spell(n));
        }
    }
}
