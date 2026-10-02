#!/bin/bash
set -e

# 6. svc.rs wildcard import
sed -i '' 's/use late_core::{db::Db, models::calendar::\*/use late_core::{db::Db, models::calendar::{CalendarStore, CalendarEvent, PublicCalendar, CalendarPreferences, CreationTier, CalendarSource, EventDraft}}/' late-ssh/src/app/calendar/svc.rs

# 7. ui.rs wildcard import
sed -i '' 's/use late_core::models::calendar::\*/use late_core::models::calendar::{CalendarSource, CalendarEvent, EventTiming, CalendarView, event_access, Occurrence}/' late-ssh/src/app/calendar/ui.rs

# 8. ui.rs grid_rule too many arguments
perl -0777 -pi -e 's/fn grid_rule\(\n    frame: &mut Frame,/#\[allow(clippy::too_many_arguments)\]\nfn grid_rule(\n    frame: &mut Frame,/g' late-ssh/src/app/calendar/ui.rs

# 9. ui.rs is_multiple_of
sed -i '' 's/if minute % 60 == 0/if minute.is_multiple_of(60)/' late-ssh/src/app/calendar/ui.rs

# 10. ui.rs vec useless
sed -i '' 's/let mut heights = vec!\[total \/ 6; 6\];/let mut heights = \[total \/ 6; 6\];/' late-ssh/src/app/calendar/ui.rs
