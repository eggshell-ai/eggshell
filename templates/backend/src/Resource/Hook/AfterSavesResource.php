<?php

namespace App\Resource\Hook;

use Doctrine\ORM\EntityManagerInterface;

/**
 * Hook interface for running custom code after a resource has been persisted.
 *
 * Implement this interface in a hook class placed under
 * src/Resource/Hooks/<ResourceName>/ to have it run automatically
 * during store and update operations, after the entity (and any child
 * line items) have been persisted and flushed to the database.
 *
 * At this point the entity has a stable primary key, so this is the correct
 * place for work that depends on the saved record existing on the database,
 * such as updating related records, cascading state, writing audit/analytics
 * rows, or triggering side effects.
 *
 * Because the entity has already been flushed, mutations performed here are
 * NOT persisted automatically; persist and flush explicitly when a hook needs
 * to store further changes.
 *
 * Note: hook classes are instantiated with no constructor arguments, so they
 * cannot use constructor dependency injection. Use the EntityManagerInterface
 * passed to this method to load and update related records.
 */
interface AfterSavesResource
{
    /**
     * Run custom logic after the entity has been persisted.
     *
     * @param object                 $entity        The entity that was just saved (has its primary key)
     * @param array                  $data          The raw request payload
     * @param string                 $action        Either 'store' or 'update'
     * @param EntityManagerInterface $entityManager The active entity manager
     */
    public function afterSave(
        object $entity,
        array $data,
        string $action,
        EntityManagerInterface $entityManager
    ): void;
}
