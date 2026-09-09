<?php

namespace App\Resource\Hook;

/**
 * Hook interface for adding custom validation to a resource.
 *
 * Implement this interface in a hook class placed under
 * src/Resource/Hooks/<ResourceName>/ to have it run automatically
 * during store and update operations, after standard entity validation.
 */
interface ValidatesResource
{
    /**
     * Validate the entity before it is persisted.
     *
     * @param object $entity The entity about to be saved (already populated with request data)
     * @param array  $data   The raw request payload
     * @param string $action Either 'store' or 'update'
     *
     * @return array<string, string|list<string>> An associative array mapping
     *         field names to one or more error messages. Return an empty
     *         array (or no entries) when validation passes.
     */
    public function validate(object $entity, array $data, string $action): array;
}
