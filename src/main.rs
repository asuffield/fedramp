use build_html::*;
use calamine::{DataType, Reader, Xlsx};
use enum_map::{Enum, EnumMap};
use lazy_regex::regex;
use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use strum::{EnumIter, IntoEnumIterator};

mod control_id;
use control_id::ControlID;

#[derive(Debug, Enum, Clone, Copy, EnumIter, PartialEq, Eq, Hash)]
enum Baselines {
    High,
    Moderate,
    Low,
}

impl Baselines {
    fn as_str(&self) -> &'static str {
        match self {
            Baselines::High => "High Baseline",
            Baselines::Moderate => "Moderate Baseline",
            Baselines::Low => "Low Baseline",
        }
    }

    fn short(&self) -> &'static str {
        match self {
            Baselines::High => "High",
            Baselines::Moderate => "Moderate",
            Baselines::Low => "Low",
        }
    }
}

#[derive(Debug, Default, Clone, PartialOrd, Ord, PartialEq, Eq)]
struct Parameters {
    assignment: String,
    additional: String,
}

impl Parameters {
    fn flatten(&self) -> Parameters {
        let ws = regex!(r"\s+");

        return Parameters {
            assignment: ws.replace_all(self.assignment.as_str(), " ").to_string(),
            additional: ws.replace_all(self.additional.as_str(), " ").to_string(),
        };
    }
}

#[derive(Debug)]
struct Control {
    id: ControlID,
    name: String,
    description: String,
    discussion: String,
    parameters: EnumMap<Baselines, Option<Parameters>>,
}
impl Control {
    fn distinct_parameters(&self) -> bool {
        let mut flat_parameters: Vec<Option<Parameters>> = self
            .parameters
            .values()
            .filter(|v| v.is_some())
            .map(|v| v.clone())
            .collect();
        for x in &mut flat_parameters {
            match x {
                Some(v) => {
                    *x = Some(v.flatten());
                },
                None => {
                    *x = Some(Parameters::default());
                }
            }
        }
        flat_parameters.sort();
        flat_parameters.dedup();
        return flat_parameters.len() > 1;
    }

    fn without_baseline(&self, level: Baselines) -> Control {
        let mut c = self.clone();
        c.parameters[level] = None;
        return c;
    }
}

impl Clone for Control {
    fn clone(&self) -> Self {
        Control{
            id: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            discussion: self.discussion.clone(),
            parameters: EnumMap::from_array(self.parameters.as_array().clone()),
        }
    }
}

impl Default for Control {
    fn default() -> Self {
        return Control {
            id: ControlID::default(),
            name: "".into(),
            description: "".into(),
            discussion: "".into(),
            parameters: EnumMap::from_fn(|_| None),
        };
    }
}

#[derive(Debug, Default)]
struct Controls {
    controls: HashMap<ControlID, Control>,
}

impl Controls {
    fn parse(sheet: calamine::Range<calamine::Data>, baseline: Baselines) -> Controls {
        let ws = regex!(r"\s+");
        let mut c = Controls::default();
        let mut header_names = HashMap::new();
        let headers = sheet.range((1, 0), (1, sheet.width().try_into().unwrap()));
        for (_, x, name) in headers.cells() {
            let str_name = name.as_string().unwrap_or_default();
            let flat_name = ws.replace_all(&str_name, " ");
            header_names.insert(x, flat_name.to_string());
        }
        for row in sheet.rows().skip(2) {
            let mut control = Control::default();
            control.parameters[baseline] = Some(Parameters::default());
            let parameters = control.parameters[baseline].as_mut().unwrap();
            for (i, v) in row.iter().enumerate() {
                if let Some(name) = header_names.get(&i) {
                    if let Some(value) = v.as_string() {
                        match name.as_str() {
                            "ID" => {
                                if let Ok(id) = value.parse::<ControlID>() {
                                    control.id = id
                                }
                            }
                            "Control Name" => control.name = value.trim().to_string(),
                            s if s.starts_with("NIST Control Description") => {
                                control.description = value.trim().to_string()
                            }
                            s if s.starts_with("NIST Discussion") => {
                                control.discussion = value.trim().to_string()
                            }
                            s if s.contains("Assignment / Selection") => {
                                parameters.assignment = value.trim().to_string()
                            }
                            s if s.contains("Additional") => {
                                parameters.additional = value.trim().to_string()
                            }
                            _ => {}
                        }
                    }
                }
            }

            if !control.id.is_empty() {
                c.controls.insert(control.id.clone(), control);
            }
        }
        return c;
    }

    fn without_baseline(&self, level: Baselines) -> Controls {
        let mut controls = HashMap::new();
        controls.extend(self.controls.iter().map(|(k, v)| (k.clone(), v.without_baseline(level))));
        return Controls{controls};
    }
}

async fn get_baselines() -> Result<HashMap<Baselines, Controls>, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let content = client.get("https://fedramp.gov/assets/resources/documents/FedRAMP_Security_Controls_Baseline.xlsx")
        .send().await?.bytes().await?;
    let buf = Cursor::new(content);
    let mut wb: Xlsx<_> = calamine::open_workbook_from_rs(buf)?;
    let mut baselines = HashMap::new();
    for baseline in Baselines::iter() {
        if let Ok(sheet) = wb.worksheet_range(baseline.as_str()) {
            baselines.insert(baseline, Controls::parse(sheet, baseline));
        }
    }
    return Ok(baselines);
}

fn merge_controls(baselines: HashMap<Baselines, Controls>) -> Controls {
    let mut all_controls = HashSet::new();
    for (_, baseline) in baselines.iter() {
        for id in baseline.controls.keys() {
            all_controls.insert(id.clone());
        }
    }

    let mut merged_controls = HashMap::new();
    for id in all_controls {
        let mut merged = Control::default();

        let high = baselines[&Baselines::High].controls.get(&id).unwrap();
        merged.id = high.id.clone();
        merged.name = high.name.to_string();
        merged.description = high.description.to_string();
        merged.discussion = high.discussion.to_string();
        for level in Baselines::iter() {
            if let Some(control) = baselines[&level].controls.get(&id) {
                merged.parameters[level] = control.parameters[level].clone();
            }
        }

        merged_controls.insert(id, merged);
    }

    return Controls {
        controls: merged_controls,
    };
}

fn tabulate_controls(controls: &Controls) -> build_html::Table {
    let mut ids: Vec<&ControlID> = controls.controls.keys().collect();
    ids.sort();
    let mut table = Table::new().with_custom_header_row(
        TableRow::new()
            .with_cell(TableCell::new(TableCellType::Header).with_raw("ID"))
            .with_cell(TableCell::new(TableCellType::Header).with_raw("H"))
            .with_cell(TableCell::new(TableCellType::Header).with_raw("M"))
            .with_cell(TableCell::new(TableCellType::Header).with_raw("L"))
            .with_cell(TableCell::new(TableCellType::Header).with_raw("Name"))
            .with_cell(TableCell::new(TableCellType::Header).with_raw("Description"))
            .with_cell(TableCell::new(TableCellType::Header).with_raw("Discussion"))
            .with_cell(TableCell::new(TableCellType::Header).with_raw("Level"))
            .with_cell(TableCell::new(TableCellType::Header).with_raw("Assignment"))
            .with_cell(TableCell::new(TableCellType::Header).with_raw("Additional guidance")),
    );
    for id in ids {
        let control = controls.controls.get(id).unwrap();

        let tick = "\u{2713}";
        let tick_if_present = |level| {
            if control.parameters[level].is_some() {
                tick
            } else {
                ""
            }
        };

        let has_parameter_rows = control.distinct_parameters();
        let rowspan = if has_parameter_rows {
            1 + control.parameters.len()
        } else {
            1
        }
        .to_string();

        let shared_cell = |content| {
            TableCell::new(TableCellType::Data)
                .with_raw(content)
                .with_attributes([("rowspan", rowspan.as_str())])
        };

        let id_str = id.to_string();
        let name_str = control.name.replace(" | ", "\n");
        let mut row = TableRow::new()
            .with_attributes([("class", "shared")])
            .with_cell(shared_cell(id_str.as_str()))
            .with_cell(shared_cell(tick_if_present(Baselines::High)))
            .with_cell(shared_cell(tick_if_present(Baselines::Moderate)))
            .with_cell(shared_cell(tick_if_present(Baselines::Low)))
            .with_cell(shared_cell(name_str.as_str()))
            .with_cell(shared_cell(control.description.as_str()))
            .with_cell(shared_cell(control.discussion.as_str()));

        if !has_parameter_rows {
            row = row.with_cell(shared_cell(""));
            if let Some(Some(parameters)) = control.parameters.values().next() {
                row = row
                    .with_cell(shared_cell(parameters.assignment.as_str()))
                    .with_cell(shared_cell(parameters.additional.as_str()))
            } else {
                row = row.with_cell(shared_cell("")).with_cell(shared_cell(""));
            }
        }

        table.add_custom_body_row(row);

        if has_parameter_rows {
            for level in Baselines::iter() {
                let mut row = TableRow::new()
                    .with_attributes([("class", format!("parameters {}", level.short()).as_str())]);
                match &control.parameters[level] {
                    Some(parameters) => {
                        row = row
                            .with_cell(TableCell::default().with_raw(level.short()))
                            .with_cell(
                                TableCell::default().with_raw(parameters.assignment.as_str()),
                            )
                            .with_cell(
                                TableCell::default().with_raw(parameters.additional.as_str()),
                            );
                    }
                    _ => {}
                }
                table.add_custom_body_row(row);
            }
        }
    }
    return table;
}

fn add_tab(html: &mut impl HtmlContainer, name: &str, title: &str, checked: bool, content: Container) {
    let input = format!(r#"<input name="tabs" type="radio" id="{name}" {} class="input"/>"#, if checked {r#"checked="checked""#} else {""});
    let label = format!(r#"<label for="{name}" class="label">{title}</label>"#);
    html.add_raw(input);
    html.add_raw(label);
    html.add_container(content.with_attributes([("class", "panel")]));
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let baselines = get_baselines().await?;
    let controls = merge_controls(baselines);
    let mut page = build_html::HtmlPage::new()
        .with_title("fedramp controls comparison")
        .with_head_link("style.css", "stylesheet");
    let mut tabs = Container::default().with_attributes([("class", "tabs")]);
    add_tab(&mut tabs, "all", "All controls", true, Container::default().with_table(tabulate_controls(&controls)));
    add_tab(&mut tabs, "high-moderate", "High-Moderate", false, Container::default().with_table(tabulate_controls(&controls.without_baseline(Baselines::Low))));
    add_tab(&mut tabs, "moderate-low", "Moderate-Low", false, Container::default().with_table(tabulate_controls(&controls.without_baseline(Baselines::High))));
    page = page.with_container(tabs);
    println!("{}", page.to_html_string());
    return Ok(());
}
