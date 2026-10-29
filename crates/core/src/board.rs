//! La pizarra de horarios de una estación: todas las paradas de todos los
//! trenes en un pueblo, leídas de los propios trenes (nunca cargadas a mano),
//! de la más temprana a la más tardía.

use crate::BASE_RATE;
use crate::{LineTrain, TimeOfDay, TownLayout, train::Run};

/// Qué tipo de servicio es un tren.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    /// El tren de la línea, que recorre la línea entera.
    Line,
    /// Una lanzadera de un ramal.
    Shuttle,
}

/// Hacia dónde va el tren de una fila, según el nombre del pueblo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bound {
    /// El tren sigue hacia este pueblo (la terminal de su recorrido).
    Towards(String),
    /// El recorrido termina en el pueblo de la propia pizarra; vino desde este.
    From(String),
}

/// Una parada de un tren en una estación.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardRow {
    /// El índice del tren entre los trenes del mundo.
    pub train: usize,
    pub service: Service,
    pub bound: Bound,
    /// Cuándo llega; `None` donde empieza el recorrido, porque ahí solo sale.
    pub arrive: Option<TimeOfDay>,
    /// Cuándo sigue viaje; `None` donde termina el recorrido, porque ahí solo
    /// llega.
    pub depart: Option<TimeOfDay>,
}

impl BoardRow {
    /// La hora por la que se lista la fila: cuándo sale o, si no, cuándo llega.
    pub fn at(&self) -> TimeOfDay {
        self.depart.or(self.arrive).unwrap_or_default()
    }
}

/// Todas las paradas en `town` de cada uno de los `trains` (primero el tren de
/// la línea, después las lanzaderas), ordenadas por hora, con las salidas
/// tomadas del horario de cada tren y los tiempos de parada de su recorrido.
/// Vacía para un pueblo donde no para ningún tren.
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
        // Tren de la línea: de bajada (Oeste -> Este) a las 16:00, de subida a
        // las 07:00; los dos paran aquí, así que cada uno tiene una llegada y
        // una salida.
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
        // Lanzadera: dos salidas hacia Norte y dos llegadas desde ahí.
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
        // Las horas salen del horario: cada salida de la lanzadera es una de
        // las listadas, y cada llegada es el tiempo del recorrido después de
        // una salida listada.
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
        // La llegada y la salida del tren de la línea son sus propios tiempos
        // de parada.
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
        // La lanzadera empieza aquí sus recorridos de subida y termina los de
        // bajada.
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
        // Una terminal de la línea: el recorrido del tren de la línea sale de
        // ahí y el otro termina ahí.
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
        // Un pueblo donde no para ningún tren tiene la pizarra vacía.
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
        // Las filas del tren de la línea son las propias paradas desplazadas:
        // el recorrido que sale del otro extremo a las 09:30 está en Medio de
        // 10:42 a 10:55.
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
