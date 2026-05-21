use std::collections::HashMap;

/// Типы атомарных данных, которые наша система способна распознать и связать
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EntityType {
    Nickname,
    Email,
    Phone,
    BankIdentifier,
    DateOfBirth,
    FullName,
}

/// Градация источников по уровню их изначальной надежности (комплаенс-класс)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceClass {
    VerifiedRegistry, // Официальные реестры, WHOIS, государственные открытые данные (Доверие: Высокое)
    PublicOSINT,      // Публичный веб, социальные сети, проверенные превью-страницы (Доверие: Среднее)
    UnverifiedDump,   // Логи компрометации, серые базы данных, текстовые пасты (Доверие: Низкое)
}

/// Метаданные источника для последующего расчета скоринга
#[derive(Debug, Clone)]
pub struct SourceMetadata {
    pub source_id: String,       // Идентификатор (например, "Maigret_DB" или "Dump_App_2025")
    pub class: SourceClass,      // Класс надежности
    pub import_timestamp: u64,   // Когда данные попали в нашу систему (Unix время)
    pub data_actual_year: u32,   // Реальный или расчетный год актуальности информации
}

/// Атомарный узел графа (конкретное значение)
#[derive(Debug, Clone)]
pub struct EntityNode {
    pub value: String,           // Само значение (например, "+79991112233" или "durov")
    pub entity_type: EntityType, // Тип данных
    pub first_seen: u64,         // Время первого обнаружения в циклах поиска
}

/// Ребро графа (связь между двумя сущностями)
#[derive(Debug, Clone)]
pub struct EntityLink {
    pub source_node_value: String, // От какой сущности идет связь
    pub target_node_value: String, // К какой сущности ведет
    pub weight_modifier: i16,      // Влияние связи на итоговый скоринг (положительное или отрицательное)
    pub metadata: SourceMetadata,  // Контекст источника, зафиксировавшего эту связь
}

/// Сводное аналитическое досье, которое компилируется на выходе из каскада
#[derive(Debug, Clone)]
pub struct IdentityProfile {
    pub root_entity: EntityNode,                 // Стартовая сущность, с которой начался поиск
    pub associated_nodes: HashMap<String, EntityNode>, // Все связанные узлы, прошедшие фильтрацию
    pub active_links: Vec<EntityLink>,           // Карта связей между узлами
    pub calculated_confidence: u8,               // Итоговый процент валидности досье (0-100%)
}