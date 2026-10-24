<?php

namespace App\Resource\Hook;

use Doctrine\ORM\EntityManagerInterface;

/**
 * Hook interface for running custom code before a resource is persisted.
 *
 * Implement this interface in a hook class placed under
 * src/Resource/Hooks/<ResourceName>/ to have it run automatically
 * during store and update operations, after standard entity validation
 * and after the entity has been populated with the request data, but
 * before the entity is persisted and flushed.
 *
 * Typical uses:
 *  - deriving/normalising values that depend on several inputs;
 *  - hashing secrets;
 *  - copying related state onto the entity;
 *  - preparing any related records that must exist alongside the entity.
 *
 * This is the auto-discovered equivalent of overriding
 * ResourceController::beforeSave() in the resource controller.
 *
 * Note: hook classes are instantiated with no constructor arguments, so they
 * cannot use constructor dependency injection. Use the EntityManagerInterface
 * passed to this method to load or prepare related records.
 */
interface BeforeSavesResource
{
    /**
     * Run custom logic before the entity is persisted.
     *
     * @param object                 $entity        The entity about to be saved (already populated with request data)
     * @param array                  $data          The raw request payload
     * @param string                 $action        Either 'store' or 'update'
     * @param EntityManagerInterface $entityManager The active entity manager
     */
    public function beforeSave(
        object $entity,
        array $data,
        string $action,
        EntityManagerInterface $entityManager
    ): void;
}
