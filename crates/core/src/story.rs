//! La historia como datos: una lista ordenada de pasos ([`Step`]). Cada paso
//! pide algo al jugador ([`Goal`]: hablar con alguien, tomar el caballo) y, al
//! cumplirlo, reproduce su escena ([`Beat`]s): líneas dichas por quien
//! corresponda, y acciones narradas que suceden en el juego ([`Action`]: le
//! dan plata, recibe o entrega algo).
//!
//! Los pasos los escribe el guion (`assets/dialogue/errand.ron`); estas son
//! solo las piezas y sus reglas. El orden y el avance los lleva
//! [`crate::Errand`].

use crate::errand::{Npc, PROTAGONIST};
use crate::{Item, Items, Money};

/// Quién dice una línea.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Speaker {
    /// El protagonista, Lisandro: sus líneas las dice el jugador.
    Protagonist,
    Npc(Npc),
}

impl Speaker {
    /// El nombre con que lo rotula un subtítulo.
    pub fn name(self) -> &'static str {
        match self {
            Self::Protagonist => PROTAGONIST,
            Self::Npc(npc) => npc.name(),
        }
    }
}

/// Algo narrado que le sucede al protagonista durante una escena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    /// Le dan plata.
    Earn(Money),
    /// Recibe algo.
    Receive(Item),
    /// Entrega algo, que ya no vuelve a sus manos.
    HandOver(Item),
}

impl Action {
    /// Aplica la acción a la plata y a las cosas del protagonista.
    pub fn apply(self, money: &mut Money, items: &mut Items) {
        match self {
            Self::Earn(amount) => money.earn(amount),
            Self::Receive(item) => items.receive(item),
            Self::HandOver(item) => {
                items.hand_over(item);
            }
        }
    }
}

/// Un momento de una escena: una línea o una acción.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Beat {
    /// `speaker` dice `text`, como subtítulo.
    Line { speaker: Speaker, text: String },
    /// Sucede una acción.
    Act(Action),
}

/// Lo que un paso de la historia pide al jugador.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Goal {
    /// Hablar con alguien, al alcance.
    Talk(Npc),
    /// Montar el caballo que presta Don Hidalgo.
    TakeHorse,
}

/// Un paso de la historia.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Step {
    pub goal: Goal,
    /// El objetivo de una línea del HUD mientras el paso está pendiente. `{town}`
    /// se reemplaza por el pueblo de la comisaría del encargo.
    pub objective: String,
    /// Lo que sucede al cumplirlo; vacío si no sucede nada.
    pub scene: Vec<Beat>,
}

impl Step {
    /// El objetivo con `{town}` reemplazado por `town`.
    pub fn objective_in(&self, town: &str) -> String {
        self.objective.replace("{town}", town)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_pay_give_and_take_away() {
        let (mut money, mut items) = (Money::ZERO, Items::default());
        Action::Earn(Money::new(2, 0)).apply(&mut money, &mut items);
        Action::Receive(Item::Letter).apply(&mut money, &mut items);
        assert_eq!(money, Money::new(2, 0));
        assert!(items.has(Item::Letter));
        Action::HandOver(Item::Letter).apply(&mut money, &mut items);
        assert!(!items.has(Item::Letter));
        assert_eq!(money, Money::new(2, 0), "the money stays");
    }
}
