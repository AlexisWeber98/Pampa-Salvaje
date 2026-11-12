//! Lo que lleva encima el protagonista, aparte de la plata: las cosas que la
//! historia le da y le quita y las que compra en la pulpería y el almacén
//! ([`crate::shop`]), cada una con su nombre, su categoría del inventario
//! ([`Category`], las del documento `mecánicas`, sección 10) y cuántas lleva.

use std::collections::BTreeMap;

/// Algo que el protagonista puede llevar encima.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Item {
    /// La carta de presentación que Don Hidalgo escribió de su puño y letra
    /// para el comisario Farach.
    Letter,
    /// Un pedazo del queso del Viejo Amadeo, regalado en su puesto.
    Cheese,
    // En la pulpería.
    CaneGlass,
    GinGlass,
    WineGlass,
    CaneBottle,
    Tobacco,
    Matches,
    Biscuit,
    // En el almacén.
    Yerba,
    Sugar,
    Jerky,
    Flour,
    Candles,
    Knife,
    Axe,
    Shovel,
    Maneador,
    Lasso,
    Alpargatas,
    Poncho,
    Revolver,
    Winchester,
    Cartridges,
}

impl Item {
    /// Todas las cosas, en el orden en que se declaran.
    pub const ALL: [Self; 24] = [
        Self::Letter,
        Self::Cheese,
        Self::CaneGlass,
        Self::GinGlass,
        Self::WineGlass,
        Self::CaneBottle,
        Self::Tobacco,
        Self::Matches,
        Self::Biscuit,
        Self::Yerba,
        Self::Sugar,
        Self::Jerky,
        Self::Flour,
        Self::Candles,
        Self::Knife,
        Self::Axe,
        Self::Shovel,
        Self::Maneador,
        Self::Lasso,
        Self::Alpargatas,
        Self::Poncho,
        Self::Revolver,
        Self::Winchester,
        Self::Cartridges,
    ];

    /// El nombre con que se muestra en el inventario.
    pub fn name(self) -> &'static str {
        match self {
            Item::Letter => "Carta de presentación",
            Item::Cheese => "Queso",
            Item::CaneGlass => "Vaso de caña",
            Item::GinGlass => "Vaso de ginebra",
            Item::WineGlass => "Vaso de vino",
            Item::CaneBottle => "Botella de caña",
            Item::Tobacco => "Tabaco y papel de armar",
            Item::Matches => "Fósforos",
            Item::Biscuit => "Galleta",
            Item::Yerba => "Yerba (1 kg)",
            Item::Sugar => "Azúcar (1 kg)",
            Item::Jerky => "Charqui (1 kg)",
            Item::Flour => "Harina (1 kg)",
            Item::Candles => "Velas",
            Item::Knife => "Cuchillo",
            Item::Axe => "Hacha",
            Item::Shovel => "Pala",
            Item::Maneador => "Maneador",
            Item::Lasso => "Lazo",
            Item::Alpargatas => "Alpargatas",
            Item::Poncho => "Poncho",
            Item::Revolver => "Revólver",
            Item::Winchester => "Winchester",
            Item::Cartridges => "Caja de 25 balas",
        }
    }

    /// La categoría del inventario donde se guarda.
    pub fn category(self) -> Category {
        match self {
            Item::Letter => Category::Quest,
            Item::Cheese | Item::Biscuit | Item::Yerba | Item::Sugar | Item::Jerky | Item::Flour => {
                Category::Food
            }
            Item::CaneGlass | Item::GinGlass | Item::WineGlass | Item::CaneBottle => Category::Drink,
            Item::Knife | Item::Axe | Item::Shovel | Item::Maneador | Item::Lasso => Category::Tools,
            Item::Alpargatas | Item::Poncho => Category::Clothing,
            Item::Revolver | Item::Winchester => Category::Weapons,
            Item::Cartridges => Category::Ammo,
            Item::Tobacco | Item::Matches | Item::Candles => Category::Sundries,
        }
    }
}

/// Una categoría del inventario, en el orden del documento `mecánicas`
/// (sección 10): las bebidas en el lugar del agua, la ropa después de las
/// herramientas y las cosas sueltas (tabaco, fósforos, velas) al final, con
/// lo obtenido explorando. Solo las que tienen algo que llevar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    /// Armas: el revólver y el Winchester.
    Weapons,
    /// Municiones: las cajas de balas.
    Ammo,
    /// Alimentos: el queso del Viejo Amadeo, la galleta, los víveres.
    Food,
    /// Bebidas: los vasos y las botellas de la pulpería.
    Drink,
    /// Herramientas: el cuchillo, el hacha, la pala, el maneador y el lazo.
    Tools,
    /// Ropa: las alpargatas y el poncho.
    Clothing,
    /// Objetos de misión: la carta de Don Hidalgo.
    Quest,
    /// Varios: el tabaco, los fósforos y las velas.
    Sundries,
}

impl Category {
    /// El título con que se muestra en el inventario.
    pub fn name(self) -> &'static str {
        match self {
            Category::Weapons => "Armas",
            Category::Ammo => "Municiones",
            Category::Food => "Alimentos",
            Category::Drink => "Bebidas",
            Category::Tools => "Herramientas",
            Category::Clothing => "Ropa",
            Category::Quest => "Objetos de misión",
            Category::Sundries => "Varios",
        }
    }
}

/// Las cosas que lleva el protagonista y cuántas de cada una.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Items(BTreeMap<Item, u32>);

impl Items {
    /// Si lleva al menos un `item`.
    pub fn has(&self, item: Item) -> bool {
        self.count(item) > 0
    }

    /// Cuántos `item` lleva.
    pub fn count(&self, item: Item) -> u32 {
        self.0.get(&item).copied().unwrap_or(0)
    }

    /// Recibe un `item` más.
    pub fn receive(&mut self, item: Item) {
        *self.0.entry(item).or_insert(0) += 1;
    }

    /// Entrega un `item` y dice si lo llevaba; si no lo llevaba, no pasa nada.
    pub fn hand_over(&mut self, item: Item) -> bool {
        match self.0.get_mut(&item) {
            Some(n) if *n > 1 => {
                *n -= 1;
                true
            }
            Some(_) => self.0.remove(&item).is_some(),
            None => false,
        }
    }

    /// Lo que lleva y cuántos, agrupado por [`Category`] en el orden del
    /// documento; las categorías sin nada no aparecen.
    pub fn by_category(&self) -> Vec<(Category, Vec<(Item, u32)>)> {
        let mut groups: Vec<(Category, Vec<(Item, u32)>)> = Vec::new();
        let mut sorted: Vec<(Item, u32)> = self.0.iter().map(|(&i, &n)| (i, n)).collect();
        sorted.sort_by_key(|(item, _)| item.category());
        for (item, n) in sorted {
            match groups.last_mut() {
                Some((category, things)) if *category == item.category() => things.push((item, n)),
                _ => groups.push((item.category(), vec![(item, n)])),
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
            vec![(Category::Quest, vec![(Item::Letter, 1)])]
        );
        items.receive(Item::Cheese);
        assert_eq!(
            items.by_category(),
            vec![
                (Category::Food, vec![(Item::Cheese, 1)]),
                (Category::Quest, vec![(Item::Letter, 1)]),
            ]
        );
    }

    #[test]
    fn items_are_counted_and_handed_over_one_at_a_time() {
        let mut items = Items::default();
        items.receive(Item::Yerba);
        items.receive(Item::Yerba);
        items.receive(Item::Revolver);
        assert_eq!(items.count(Item::Yerba), 2);
        assert_eq!(
            items.by_category(),
            vec![
                (Category::Weapons, vec![(Item::Revolver, 1)]),
                (Category::Food, vec![(Item::Yerba, 2)]),
            ]
        );
        assert!(items.hand_over(Item::Yerba));
        assert_eq!(items.count(Item::Yerba), 1);
        assert!(items.hand_over(Item::Yerba));
        assert!(!items.has(Item::Yerba) && !items.hand_over(Item::Yerba));
        assert_eq!(
            items.by_category(),
            vec![(Category::Weapons, vec![(Item::Revolver, 1)])]
        );
    }
}
