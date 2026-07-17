//! SDMX-ML XML parser (structure and data).

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::error::{Error, Result};
use crate::model::{Dimension, DimensionCode, Observation, ObservationTable};

/// Parse SDMX-ML structure into dimensions.
pub fn parse_dimensions(bytes: &[u8]) -> Result<Vec<Dimension>> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);

    let mut dimensions = Vec::new();
    let mut current_dim: Option<Dimension> = None;
    let mut in_code = false;
    let mut current_code_id = String::new();
    let mut current_code_label = None::<String>;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                match name.as_str() {
                    "Dimension" | "DimensionReference" => {
                        let id = attribute(&e, "id").unwrap_or_default();
                        current_dim = Some(Dimension {
                            id,
                            label: None,
                            codes: Vec::new(),
                        });
                    }
                    "Code" => {
                        in_code = true;
                        current_code_id = attribute(&e, "id").unwrap_or_default();
                        current_code_label = None;
                    }
                    "Name" if in_code => {}
                    _ => {}
                }
            }
            Ok(Event::Text(text)) => {
                if in_code {
                    current_code_label = Some(text.unescape().map_err(xml_err)?.into_owned());
                } else if let Some(dim) = current_dim.as_mut() {
                    if dim.label.is_none() {
                        dim.label = Some(text.unescape().map_err(xml_err)?.into_owned());
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                match name.as_str() {
                    "Code" => {
                        if let Some(dim) = current_dim.as_mut() {
                            dim.codes.push(DimensionCode {
                                id: current_code_id.clone(),
                                label: current_code_label.clone(),
                            });
                        }
                        in_code = false;
                    }
                    "Dimension" | "DimensionReference" => {
                        if let Some(dim) = current_dim.take() {
                            if !dim.id.is_empty() {
                                dimensions.push(dim);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(xml_err(err)),
            _ => {}
        }
        buf.clear();
    }

    Ok(dimensions)
}

/// Parse SDMX-ML generic data into observations.
pub fn parse_data(bytes: &[u8], dataset_id: &str) -> Result<ObservationTable> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);

    let mut observations = Vec::new();
    let mut dims = Vec::new();
    let mut value = None;
    let mut in_obs = false;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                match name.as_str() {
                    "Obs" => {
                        in_obs = true;
                        dims.clear();
                        value = None;
                    }
                    "ObsDimension" if in_obs => {
                        let dim_id = attribute(&e, "id").unwrap_or_default();
                        let dim_value = attribute(&e, "value").unwrap_or_default();
                        dims.push((dim_id, dim_value));
                    }
                    "ObsValue" if in_obs => {
                        value = attribute(&e, "value").and_then(|v| v.parse().ok());
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if name == "Obs" && in_obs {
                    observations.push(Observation {
                        dimensions: dims.clone(),
                        value,
                        status: None,
                    });
                    in_obs = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(xml_err(err)),
            _ => {}
        }
        buf.clear();
    }

    Ok(ObservationTable {
        dataset_id: dataset_id.to_string(),
        title: None,
        dimensions: Vec::new(),
        observations,
    })
}

fn attribute(event: &quick_xml::events::BytesStart<'_>, key: &str) -> Option<String> {
    for attr in event.attributes().flatten() {
        if attr.key.as_ref() == key.as_bytes() {
            return String::from_utf8(attr.value.into_owned()).ok();
        }
    }
    None
}

fn xml_err<E: std::fmt::Display>(err: E) -> Error {
    Error::Xml(err.to_string())
}
