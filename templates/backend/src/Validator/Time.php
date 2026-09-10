<?php

namespace App\Validator;

use Symfony\Component\Validator\Constraint;

#[\Attribute]
class Time extends Constraint
{
    public string $message = 'The value "{{ string }}" is not a valid time. It must be in HH:MM:SS format (e.g. 14:30:00).';

    // all configurable options must be passed to the constructor
    public function __construct(?string $message = null, ?array $groups = null, $payload = null)
    {
        $this->message = $message ?? $this->message;

        parent::__construct(null, $groups, $payload);
    }
}
