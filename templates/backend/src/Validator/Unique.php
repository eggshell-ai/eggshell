<?php

namespace App\Validator;

use Symfony\Component\Validator\Constraint;

#[\Attribute]
class Unique extends Constraint
{
    public string $message = 'The value "{{ string }}" is already in use.';

    // all configurable options must be passed to the constructor
    public function __construct(?string $message = null, ?array $groups = null, $payload = null)
    {
        $this->message = $message ?? $this->message;

        parent::__construct(null, $groups, $payload);
    }
}
