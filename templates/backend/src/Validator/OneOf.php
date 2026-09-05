<?php

namespace App\Validator;

use Symfony\Component\Validator\Constraint;

#[\Attribute]
class OneOf extends Constraint
{
    public array $choices = [];
    public string $message = 'The value "{{ string }}" is not valid. It must be one of: {{ choices }}.';

    // all configurable options must be passed to the constructor
    public function __construct(?array $choices = null, ?string $message = null, ?array $groups = null, $payload = null)
    {
        $this->choices = $choices ?? $this->choices;
        $this->message = $message ?? $this->message;

        parent::__construct(null, $groups, $payload);
    }
}
