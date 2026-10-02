#!/bin/bash
set -e

# 1. input.rs wildcard import
sed -i '' 's/use late_core::models::calendar::\*/use late_core::models::calendar::{CalendarView, CalendarSource, CreationTier, EventAccess, event_access, Occurrence}/' late-ssh/src/app/calendar/input.rs

# 2. input.rs collapsible if 1
perl -0777 -pi -e 's/    if let Some\(Modal::Editor\(e\)\) = &mut s\.modal \{\n        if e\.dirty\(\) \{\n            e\.discard_prompt = true;\n            return;\n        \}\n    \}/    if let Some(Modal::Editor(e)) = &mut s.modal\n        && e.dirty()\n    {\n        e.discard_prompt = true;\n        return;\n    }/g' late-ssh/src/app/calendar/input.rs

# 3. input.rs collapsible if 2
perl -0777 -pi -e 's/            if let Some\(Modal::Editor\(e\)\) = &s\.modal \{\n                if let Some\(\(id, _\)\) = e\.existing \{\n                    s\.service_reload\(id\);\n                \}\n            \}/            if let Some(Modal::Editor(e)) = &s.modal\n                && let Some((id, _)) = e.existing\n            {\n                s.service_reload(id);\n            }/g' late-ssh/src/app/calendar/input.rs

# 4. state.rs wildcard import
sed -i '' 's/use late_core::models::calendar::\*/use late_core::models::calendar::{CalendarPreferences, CalendarEvent, CalendarSource, Occurrence, EventAccess, CreationTier, event_access, EventTiming, EventDraft, local_instant, CalendarView, PublicCalendar}/' late-ssh/src/app/calendar/state.rs

# 5. state.rs Modal::Go Box
sed -i '' 's/Go(TextArea<'"'static"'>)/Go(Box<TextArea<'"'static"'>})/' late-ssh/src/app/calendar/state.rs
# Update usages of Modal::Go in input.rs
sed -i '' 's/Modal::Go(TextArea::default())/Modal::Go(Box::new(TextArea::default()))/' late-ssh/src/app/calendar/input.rs
# Update usages of Modal::Go in ui_test.rs
sed -i '' 's/Modal::Go(ratatui_textarea::TextArea::from/Modal::Go(Box::new(ratatui_textarea::TextArea::from/' late-ssh/src/app/calendar/ui_test.rs
# Add closing paren in ui_test.rs where appropriate. Wait, ui_test.rs:59 is over multiple lines. Let's look at ui_test.rs:59 first.
