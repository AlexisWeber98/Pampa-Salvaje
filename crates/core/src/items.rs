//! Lo que lleva encima el protagonista, aparte de la plata: unas pocas cosas
//! que la historia le da y le quita, cada una con su nombre y su categoría del
//! inventario ([`Category`], las del documento `mecánicas`, sección 10).

use std::collections::BTreeSet;

/// Algo que el protagonista puede llevar encima.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Item {
    /// La carta de presentación que Don Hidalgo escribió de su puño y letra
    /// para el comisario Farach.
    Letter,
    /// Un pedazo del queso del Viejo Amadeo, regalado en su puesto.
    Cheese,
}

impl Item {
    /// El nombre con que se muestra en el inventario.
    pub fn name(self) -> &'static str {
        match self {
            Item::Letter => "Carta de presentación",
            Item::Cheese => "Queso",
        }
    }

    /// La categoría del inventario donde se guarda.
    pub fn category(self) -> Category {
        match self {
            Item::Letter => Category::Quest,
            Item::Cheese => Category::Food,
        }
    }
}

/// Una categoría del inventario, en el orden del documento `mecánicas`
/// (sección 10). Por ahora solo las que tienen algo que llevar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    /// Alimentos: el queso del Viejo Amadeo.
    Food,
    /// Objetos de misión: la carta de Don Hidalgo.
    Quest,
}

impl Category {
    /// El título con que se muestra en el inventario.
    pub fn name(self) -> &'static str {
        match self {
            Category::Food => "Alimentos",
            Category::Quest => "Objetos de misión",
        }
    }
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

    /// Lo que lleva, agrupado por [`Category`] en el orden del documento; las
    /// categorías sin nada no aparecen.
    pub fn by_category(&self) -> Vec<(Category, Vec<Item>)> {
        let mut groups: Vec<(Category, Vec<Item>)> = Vec::new();
        let mut sorted: Vec<Item> = self.0.iter().copied().collect();
        sorted.sort_by_key(|item| item.category());
        for item in sorted {
            match groups.last_mut() {
                Some((category, things)) if *category == item.category() => things.push(item),
                _ => groups.push((item.category(), vec![item])),
            }
        }
        groups
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_item_has_its_name_and_its_inventory_category() {
        assert_eq!(Item::Cheese.name(), "Queso");
        assert_eq!(Item::Cheese.category(), Category::Food);
        assert_eq!(Item::Letter.name(), "Carta de presentación");
        assert_eq!(Item::Letter.category(), Category::Quest);
        assert_eq!(Category::Food.name(), "Alimentos");
        assert_eq!(Category::Quest.name(), "Objetos de misión");
    }

    #[test]
    fn items_group_by_category_in_the_design_order_without_empty_ones() {
        let mut items = Items::default();
        assert!(items.by_category().is_empty());
        items.receive(Item::Letter);
        assert_eq!(
            items.by_category(),
            vec![(Category::Quest, vec![Item::Letter])]
        );
        items.receive(Item::Cheese);
        assert_eq!(
            items.by_category(),
            vec![
                (Category::Food, vec![Item::Cheese]),
                (Category::Quest, vec![Item::Letter]),
            ]
        );
    }
}
