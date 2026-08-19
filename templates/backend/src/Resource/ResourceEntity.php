<?php

namespace App\Resource;

use App\Resource\Attribute\Table;
use App\Resource\Attribute\Form;
use ReflectionClass;

abstract class ResourceEntity
{
    /**
     * @return string A string representation of the title / label of the current record
     */
    abstract public function getTitle(): string;

    public function table(): array
    {
        $reflection = new ReflectionClass($this);
        $fields = [];

        foreach ($reflection->getProperties() as $property) {
            $attributes = $property->getAttributes(Table::class);
            if (!empty($attributes)) {
                $fields[$property->getName()] = $attributes[0]->newInstance();
            }
        }

        return $fields;
    }

    public function form(): array
    {
        $reflection = new ReflectionClass($this);
        $fields = [];

        foreach ($reflection->getProperties() as $property) {
            $attributes = $property->getAttributes(Form::class);
            if (!empty($attributes)) {
                $fields[$property->getName()] = $attributes[0]->newInstance();
            }
        }

        return $fields;
    }
}
