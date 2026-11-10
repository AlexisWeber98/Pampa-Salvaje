//! Lo que lleva encima el protagonista, aparte de la plata: unas pocas cosas
//! que la historia le da y le quita. No hay inventario a la vista; solo se
//! lleva la cuenta de qué tiene.

use std::collections::BTreeSet;

/// Algo que el protagonista puede llevar encima.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Item {
    /// La carta de presentación que Don Hidalgo escribió de su puño y letra
    /// para el comisario Farach.
    Letter,
}

/// Las cosas que lleva el protagonista, cada una como mucho una vez.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Items(BTreeSet<Item>);

impl Items {
    /// Si lleva `item`.
    pub fn has(&self, item: Item) -> bool {
        self.0.contains(&item)
    }

    /// Recibe `item`.
    pub fn receive(&mut self, item: Item) {
        self.0.insert(item);
    }

    /// Entrega `item` y dice si lo llevaba; si no lo llevaba, no pasa nada.
    pub fn hand_over(&mut self, item: Item) -> bool {
        self.0.remove(&item)
    }
}
