#!/bin/bash
# Builds the made-up home folder the README GIF is recorded in: make-home.sh DIR
set -euo pipefail
H=$1
rm -rf "$H"; mkdir -p "$H"
cd "$H"

# The project the GIF opens in
P=Projects/weather-app
mkdir -p $P/src $P/assets/icons $P/docs
cat > $P/Cargo.toml <<'T'
[package]
name = "weather-app"
version = "0.3.0"
edition = "2021"

[dependencies]
reqwest = { version = "0.12", features = ["json"] }
serde = { version = "1", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
T
cat > $P/README.md <<'T'
# weather-app

A tiny CLI that prints the forecast for any city.

## Usage

```sh
weather-app "Lisbon" --days 3
```

## Features

- Hourly and daily forecasts
- Celsius or Fahrenheit
- Caches responses for 10 minutes
T
cat > $P/src/main.rs <<'T'
use std::env;

mod api;
mod forecast;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let city = env::args().nth(1).unwrap_or_else(|| "Lisbon".into());
    let days: u8 = env::args().nth(2).and_then(|d| d.parse().ok()).unwrap_or(3);

    let report = api::fetch(&city, days).await?;
    for day in forecast::summarize(&report) {
        println!("{:<10} {:>4}°C  {}", day.name, day.high, day.sky);
    }
    Ok(())
}
T
cat > $P/src/api.rs <<'T'
use serde::Deserialize;

const BASE_URL: &str = "https://api.open-meteo.com/v1/forecast";

#[derive(Debug, Deserialize)]
pub struct Report {
    pub daily: Daily,
}

#[derive(Debug, Deserialize)]
pub struct Daily {
    pub time: Vec<String>,
    pub temperature_2m_max: Vec<f32>,
    pub weather_code: Vec<u8>,
}

pub async fn fetch(city: &str, days: u8) -> reqwest::Result<Report> {
    let (lat, lon) = crate::forecast::coordinates(city);
    let url = format!("{BASE_URL}?latitude={lat}&longitude={lon}&forecast_days={days}");
    reqwest::get(url).await?.json().await
}
T
cat > $P/src/forecast.rs <<'T'
use crate::api::Report;

pub struct Day {
    pub name: String,
    pub high: i32,
    pub sky: &'static str,
}

fn sky(code: u8) -> &'static str {
    match code {
        0 => "clear",
        1..=3 => "cloudy",
        51..=67 => "rain",
        _ => "mixed",
    }
}

pub fn summarize(report: &Report) -> Vec<Day> {
    let daily = &report.daily;
    daily.time.iter().enumerate().map(|(i, date)| Day {
        name: date.clone(),
        high: daily.temperature_2m_max[i].round() as i32,
        sky: sky(daily.weather_code[i]),
    }).collect()
}
T
printf 'target/\n.env\n' > $P/.gitignore
printf '# Architecture\n\nThe CLI calls Open-Meteo and prints one line per day.\n' > $P/docs/architecture.md
: > $P/assets/icons/sun.svg; : > $P/assets/icons/cloud.svg
printf 'MIT License\n\nCopyright (c) 2026\n' > $P/LICENSE

# Other projects
for proj in blog portfolio dotfiles budget-tracker ml-notebooks; do
  mkdir -p Projects/$proj/{docs,scripts,assets/images,tests}
  printf '# %s\n' "$proj" > Projects/$proj/README.md
done

# The deeply nested client folders (the global search target)
for client in Northwind Acme Globex Initech Umbrella; do
  for year in 2024 2025 2026; do
    for q in Q1 Q2 Q3 Q4; do
      mkdir -p "Documents/Work/Clients/$client/Projects/$year/$q/"{Invoices,Meetings,Contracts/drafts}
    done
  done
  mkdir -p "Documents/Work/Clients/$client/Archive"
done
S="Documents/Work/Clients/Northwind/Projects/2026/Q3/Contracts/signed"
mkdir -p "$S"
# One-page PDFs with text, so the preview has something to show
pdf() {
  python3 -I - "$@" <<'PY'
import sys

out, lines = sys.argv[1], sys.argv[2:]
text = "BT /F1 14 Tf 72 720 Td 20 TL " + " ".join(
    "(%s) Tj T*" % l.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)") for l in lines
) + " ET"
objs = [
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
    "<< /Length %d >>\nstream\n%s\nendstream" % (len(text), text),
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
]
body, offsets = "%PDF-1.4\n", []
for i, o in enumerate(objs, 1):
    offsets.append(len(body))
    body += "%d 0 obj\n%s\nendobj\n" % (i, o)
xref = len(body)
body += "xref\n0 %d\n0000000000 65535 f \n" % (len(objs) + 1)
body += "".join("%010d 00000 n \n" % o for o in offsets)
body += "trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (len(objs) + 1, xref)
open(out, "w").write(body)
PY
}
pdf "$S/statement-of-work.pdf" "Statement of Work" "Northwind Traders - Q3 2026" "" "Scope: redesign the customer portal" "Timeline: July 1 - September 30" "Budget: 48,000 USD" "" "Signed by both parties on June 24, 2026."
pdf "$S/master-services-agreement.pdf" "Master Services Agreement" "Northwind Traders" "" "Term: 24 months" "Payment: net 30"
pdf "$S/nda-northwind.pdf" "Mutual Non-Disclosure Agreement" "Northwind Traders" "" "Effective: January 5, 2026"

# Web projects, each with its own components folders: the many same-named folders
# that the multi-word search narrows down
for proj in storefront admin-panel landing-page docs-site dashboard checkout-flow \
    design-system booking-app crm-portal support-desk recipe-box travel-planner \
    chat-client music-player invoice-app kanban-board photo-gallery fitness-tracker; do
  mkdir -p Projects/$proj/{src/{components,pages,hooks,utils,styles,lib,api,types},src/features/{auth,settings}/components,public/images,tests/{unit,e2e},docs}
done
for proj in map-viewer field-survey data-portal permits-app transit-tracker \
    parcel-search story-maps site-planner; do
  mkdir -p work/$proj/{app/{components,routes,hooks,lib,styles},app/features/{layers,search}/components,tests,docs}
done
G=work/geoid-ai
mkdir -p $G/app/{components,routes,hooks,lib,styles} $G/{tests,docs}
for c in MapView LayerList SearchBar Legend; do
  printf 'export function %s() {\n  return null;\n}\n' "$c" > "$G/app/components/$c.tsx"
done

# ff itself, found by its initials
F=Projects/file-finder-rust
mkdir -p $F/{src/{app,config,event},tests,assets/demo}
printf '[package]\nname = "ff"\nversion = "0.3.0"\nedition = "2021"\n' > $F/Cargo.toml
printf '# ff\n' > $F/README.md

# Everyday folders
mkdir -p Documents/Personal/{Taxes/{2023,2024,2025}/receipts,Medical,Recipes,Travel/{Lisbon,Tokyo,Mexico-City}}
mkdir -p Desktop/Screenshots Downloads/{installers,invoices} Music/{Playlists,Podcasts}
mkdir -p Pictures/Photos/{2016..2026}/{01..12}/{01..28}
mkdir -p Documents/Archive/Mail/{2006..2026}/{01..12}/{01..28}
for a in Radiohead Bjork "Daft Punk" Portishead Khruangbin; do
  for al in "Album One" "Album Two" "Live"; do mkdir -p "Music/$a/$al"; done
done

# A Go module cache: about 19,000 folders nobody searches, for the index manager to
# switch off
for org in {a..t}; do
  for repo in {1..9}; do
    mkdir -p "go/pkg/mod/github.com/org-$org/module-$repo@v1.$repo.0"/{internal,pkg,cmd,api,examples,testdata,docs,scripts,tools,v2}/{auth,cache,config,codec,encoding,errors,http,log,metrics,proto}
  done
done

# ff settings
mkdir -p .config/ff
cat > .config/ff/settings.toml <<T
start_path = "$H/Projects"
root_dir = "$H"
use_nerd_fonts = "always"
T
