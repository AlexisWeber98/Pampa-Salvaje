//! The timetable board of a station: every stop of every train at a town,
//! read off the trains themselves (never typed in), earliest first.

use crate::BASE_RATE;
use crate::{LineTrain, TimeOfDay, TownLayout, train::Run};

/// What kind of service a train is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    /// The line train, which runs the whole line.
    Line,
    /// A branch shuttle.
    Shuttle,
}

/// Which way a row's train goes, by town name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bound {
    /// The train carries on toward this town (the terminal of its run).
    Towards(String),
    /// The run ends at the board's own town; it came from this one.
    From(String),
}

/// One stop of one train at a station.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardRow {
    /// The train's index among the world's trains.
    pub train: usize,
    pub service: Service,
    pub bound: Bound,
    /// When it comes in; `None` where the run starts, so it only leaves.
    pub arrive: Option<TimeOfDay>,
    /// When it goes on; `None` where the run ends, so it only arrives.
    pub depart: Option<TimeOfDay>,
}

impl BoardRow {
    /// The time the row is listed by: when it leaves, else when it comes in.
    pub fn at(&self) -> TimeOfDay {
        self.depart.or(self.arrive).unwrap_or_default()
    }
}

/// Every stop at `town` of each of `trains` (the line train first, then the
/// shuttles), sorted by time, with the departures taken from each train's
/// timetable and the stop times from its run. Empty for a town no train
/// stops at.
pub fn station_board(trains: &[LineTrain], towns: &[TownLayout], town: usize) -> Vec<BoardRow> {
    let name = |t: usize| towns.get(t).map_or_else(String::new, |t| t.name.clone());
    let mut rows = Vec::new();
    for (index, train) in trains.iter().enumerate() {
        let (Some(station), Some(first), Some(last)) = (
            train.station_of_town(town),
            train.stations().first(),
            train.stations().last(),
        ) else {
            continue;
        };
        let service = if index == 0 {
            Service::Line
        } else {
            Service::Shuttle
        };
        for run in [Run::Down, Run::Up] {
            let Some((arrive, depart)) = train.stop_times(run, station) else {
                continue;
            };
            let (origin, end) = match run {
                Run::Down => (first.town, last.town),
                Run::Up => (last.town, first.town),
            };
            let bound = if town == end {
                Bound::From(name(origin))
            } else {
                Bound::Towards(name(end))
            };
            let mut starts: Vec<f64> = train
                .timetable()
                .departures(run)
                .iter()
                .map(|at| at.seconds())
                .collect();
            starts.sort_by(f64::total_cmp);
            for start in starts {
                let at = |secs: f64| TimeOfDay::from_seconds(start + secs * BASE_RATE);
                rows.push(BoardRow {
                    train: index,
                    service,
                    bound: bound.clone(),
                    arrive: (town != origin).then(|| at(arrive)),
                    depart: (town != end).then(|| at(depart)),
                });
            }
        }
    }
    rows.sort_by(|a, b| a.at().seconds().total_cmp(&b.at().seconds()));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Timetable;
    use crate::train::line_tests::{MEDIO, branched_trains};

    const NORTE: usize = 3;

    fn hm(h: u32, m: u32) -> TimeOfDay {
        TimeOfDay::hm(h, m)
    }

    #[test]
    fn medio_lists_both_line_runs_and_both_shuttle_round_trips() {
        let (towns, line, shuttle) = branched_trains(Timetable::default());
        let rows = station_board(&[line.clone(), shuttle.clone()], &towns, MEDIO);
        let of = |train: usize| rows.iter().filter(move |r| r.train == train);
        // Line train: down (Oeste -> Este) at 16:00, up at 07:00; each
        // stops here, so each has both an arrival and a departure.
        assert_eq!(of(0).count(), 2);
        for row in of(0) {
            assert_eq!(row.service, Service::Line);
            assert!(row.arrive.is_some() && row.depart.is_some());
        }
        let line_bound = |run: Run| match run {
            Run::Down => Bound::Towards(towns[line.stations().last().unwrap().town].name.clone()),
            Run::Up => Bound::Towards(towns[line.stations()[0].town].name.clone()),
        };
        let line_bounds: Vec<&Bound> = of(0).map(|r| &r.bound).collect();
        assert!(line_bounds.contains(&&line_bound(Run::Down)));
        assert!(line_bounds.contains(&&line_bound(Run::Up)));
        assert_ne!(line_bound(Run::Down), line_bound(Run::Up));
        // Shuttle: two departures to Norte and two arrivals from it.
        let leaving: Vec<_> = of(1).filter(|r| r.depart.is_some()).collect();
        let coming: Vec<_> = of(1).filter(|r| r.depart.is_none()).collect();
        assert_eq!((leaving.len(), coming.len()), (2, 2));
        let norte = towns[NORTE].name.clone();
        assert!(
            leaving
                .iter()
                .all(|r| r.bound == Bound::Towards(norte.clone()))
        );
        assert!(leaving.iter().all(|r| r.arrive.is_none()));
        assert!(coming.iter().all(|r| r.bound == Bound::From(norte.clone())));
        assert!(coming.iter().all(|r| r.service == Service::Shuttle));
        // Times come from the timetable: each shuttle departure is a listed
        // one, and each arrival is the run's time after a listed departure.
        let listed: Vec<f64> = shuttle
            .timetable()
            .down
            .iter()
            .map(|t| t.seconds())
            .collect();
        for row in &leaving {
            assert!(listed.contains(&row.depart.unwrap().seconds()));
        }
        let up_arrivals = shuttle.arrivals_at(Run::Up, 0);
        for row in &coming {
            assert!(up_arrivals.contains(&row.arrive.unwrap()));
        }
        // The line train's arrival and departure are its own stop times.
        let station = line.station_of_town(MEDIO).unwrap();
        for run in [Run::Down, Run::Up] {
            let arrivals = line.arrivals_at(run, station);
            assert!(of(0).any(|r| arrivals.contains(&r.arrive.unwrap())));
        }
    }

    #[test]
    fn the_branch_end_lists_only_the_shuttle_and_a_line_station_only_the_line() {
        let (towns, line, shuttle) = branched_trains(Timetable::default());
        let trains = [line, shuttle];
        let end = station_board(&trains, &towns, NORTE);
        assert_eq!(end.len(), 4);
        assert!(
            end.iter()
                .all(|r| r.train == 1 && r.service == Service::Shuttle)
        );
        // The shuttle starts its up runs here and ends its down runs.
        assert_eq!(end.iter().filter(|r| r.arrive.is_none()).count(), 2);
        assert_eq!(end.iter().filter(|r| r.depart.is_none()).count(), 2);
        assert!(
            end.iter()
                .any(|r| r.bound == Bound::Towards(towns[MEDIO].name.clone()))
        );
        assert!(
            end.iter()
                .any(|r| r.bound == Bound::From(towns[MEDIO].name.clone()))
        );
        // A terminal of the line: the line train's run leaves it and the
        // other ends there.
        for town in [0, 2] {
            let rows = station_board(&trains, &towns, town);
            assert_eq!(rows.len(), 2, "town {town}");
            assert!(
                rows.iter()
                    .all(|r| r.train == 0 && r.service == Service::Line)
            );
            assert_eq!(rows.iter().filter(|r| r.arrive.is_none()).count(), 1);
            assert_eq!(rows.iter().filter(|r| r.depart.is_none()).count(), 1);
        }
        // A town no train stops at has an empty board.
        assert!(station_board(&trains, &towns, 99).is_empty());
    }

    #[test]
    fn rows_are_sorted_and_follow_the_timetables_when_they_shift() {
        let (towns, line, shuttle) = branched_trains(Timetable::default());
        let rows = station_board(&[line, shuttle], &towns, MEDIO);
        assert_eq!(rows.len(), 6);
        assert!(
            rows.windows(2)
                .all(|w| w[0].at().seconds() <= w[1].at().seconds())
        );

        let shifted = Timetable {
            up: vec![hm(9, 30)],
            down: vec![hm(18, 0)],
            ..Timetable::default()
        };
        let (_, line2, shuttle2) = branched_trains(shifted);
        let moved = station_board(&[line2, shuttle2], &towns, MEDIO);
        assert_eq!(moved.len(), 6);
        assert!(
            moved
                .windows(2)
                .all(|w| w[0].at().seconds() <= w[1].at().seconds())
        );
        let times = |rows: &[BoardRow], train: usize| -> Vec<f64> {
            rows.iter()
                .filter(|r| r.train == train)
                .map(|r| r.at().seconds())
                .collect()
        };
        for train in [0, 1] {
            assert_ne!(times(&rows, train), times(&moved, train), "train {train}");
        }
        // The line train's rows are the shifted stops themselves: the run
        // that leaves the far end at 09:30 is at Medio 10:42 to 10:55.
        let shown: Vec<(String, String)> = moved
            .iter()
            .filter(|r| r.train == 0)
            .map(|r| (r.arrive.unwrap().to_string(), r.depart.unwrap().to_string()))
            .collect();
        assert_eq!(
            shown,
            [
                ("10:42".into(), "10:55".into()),
                ("19:12".into(), "19:25".into())
            ]
        );
    }
}
